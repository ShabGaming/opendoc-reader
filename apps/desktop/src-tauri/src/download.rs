//! Resumable, verified file downloads (M1-3).
//!
//! One file at a time: [`Downloader::download`] tries the mirrors in the manifest's order,
//! resumes a partial `.part` file with an HTTP `Range` request, checks the SHA-256 and only then
//! renames the file into place, so a path that exists is a path that was verified. The caller
//! bounds concurrency across files and aggregates progress.
//!
//! Proxies: the client is built with reqwest's default proxy handling, which reads
//! `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` and, on Windows and macOS, the system proxy settings.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::contract::{AppError, AppErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    Paused,
    Cancelled,
}

/// Pause, resume and cancel for a group of downloads. Clones share the same state.
#[derive(Clone)]
pub struct Control(Arc<watch::Sender<RunState>>);

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

impl Control {
    pub fn new() -> Self {
        Self(Arc::new(watch::channel(RunState::Running).0))
    }

    pub fn state(&self) -> RunState {
        *self.0.borrow()
    }

    /// Pausing keeps the partial files; a cancelled control cannot be revived.
    pub fn pause(&self) {
        self.0.send_if_modified(|state| {
            if *state == RunState::Running {
                *state = RunState::Paused;
                true
            } else {
                false
            }
        });
    }

    pub fn resume(&self) {
        self.0.send_if_modified(|state| {
            if *state == RunState::Paused {
                *state = RunState::Running;
                true
            } else {
                false
            }
        });
    }

    pub fn cancel(&self) {
        self.0.send_replace(RunState::Cancelled);
    }

    fn subscribe(&self) -> watch::Receiver<RunState> {
        self.0.subscribe()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FileEvent {
    /// Bytes of this file on disk so far, counting a resumed prefix.
    Bytes(u64),
    /// All bytes arrived; the checksum is being computed.
    Verifying,
}

pub type Notify<'a> = &'a (dyn Fn(FileEvent) + Send + Sync);

#[derive(Debug, Clone)]
pub struct DownloadFile {
    pub dest: PathBuf,
    /// Mirrors in the order to try them; all serve the same bytes.
    pub urls: Vec<String>,
    pub sha256: String,
    pub size: u64,
}

pub struct Downloader {
    client: reqwest::Client,
    retry_delay: Duration,
    attempts_per_url: u32,
}

/// Why one attempt on one URL stopped.
enum Fail {
    Cancelled,
    Fatal(AppError),
    /// No answer from the server at all (DNS, refused, timeout).
    Connect(String),
    /// The transfer broke off after the server had answered.
    Stream(String),
    Http(u16),
    Mismatch,
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

impl Downloader {
    pub fn new() -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("OpenDocReader/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| {
                AppError::internal("Could not create the HTTP client.").with_detail(e.to_string())
            })?;
        Ok(Self {
            client,
            retry_delay: Duration::from_secs(2),
            attempts_per_url: 3,
        })
    }

    #[cfg(test)]
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// Downloads one file. A file already at `dest` with the expected size is trusted, because
    /// files only reach `dest` after their checksum was verified.
    pub async fn download(
        &self,
        file: &DownloadFile,
        control: &Control,
        notify: Notify<'_>,
    ) -> Result<(), AppError> {
        if let Ok(meta) = tokio::fs::metadata(&file.dest).await {
            if meta.len() == file.size {
                notify(FileEvent::Bytes(file.size));
                return Ok(());
            }
        }
        if let Some(parent) = file.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut saw_response = false;
        let mut last: Option<Fail> = None;
        for url in &file.urls {
            for attempt in 0..self.attempts_per_url {
                if attempt > 0 {
                    self.sleep_unless_cancelled(control, self.retry_delay * attempt)
                        .await?;
                }
                match self
                    .attempt(url, file, control, notify, &mut saw_response)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(Fail::Cancelled) => return Err(AppError::cancelled()),
                    Err(Fail::Fatal(error)) => return Err(error),
                    Err(fail @ (Fail::Mismatch | Fail::Http(_))) => {
                        tracing::warn!("download from a mirror failed: {}", describe(&fail));
                        last = Some(fail);
                        break;
                    }
                    Err(fail) => {
                        tracing::warn!("download interrupted: {}", describe(&fail));
                        last = Some(fail);
                    }
                }
            }
        }
        Err(final_error(last, saw_response))
    }

    async fn sleep_unless_cancelled(
        &self,
        control: &Control,
        delay: Duration,
    ) -> Result<(), AppError> {
        let mut rx = control.subscribe();
        tokio::select! {
            _ = tokio::time::sleep(delay) => Ok(()),
            _ = rx.wait_for(|s| *s == RunState::Cancelled) => Err(AppError::cancelled()),
        }
    }

    async fn attempt(
        &self,
        url: &str,
        file: &DownloadFile,
        control: &Control,
        notify: Notify<'_>,
        saw_response: &mut bool,
    ) -> Result<(), Fail> {
        let part = part_path(&file.dest);
        let mut offset = match tokio::fs::metadata(&part).await {
            Ok(meta) => meta.len(),
            Err(_) => 0,
        };
        if offset > file.size {
            let _ = tokio::fs::remove_file(&part).await;
            offset = 0;
        }
        let mut hasher = Sha256::new();
        if offset > 0 {
            offset = hash_prefix(&part, offset, &mut hasher)
                .await
                .map_err(|e| Fail::Fatal(e.into()))?;
        }
        notify(FileEvent::Bytes(offset));

        let mut rx = control.subscribe();
        'transfer: while offset < file.size {
            wait_runnable(&mut rx).await?;

            let mut request = self.client.get(url);
            if offset > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let mut response = request.send().await.map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    Fail::Connect(e.to_string())
                } else {
                    Fail::Stream(e.to_string())
                }
            })?;
            *saw_response = true;

            match response.status().as_u16() {
                206 => {
                    let start = response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .and_then(content_range_start);
                    if start != Some(offset) {
                        return Err(Fail::Stream("unexpected Content-Range".into()));
                    }
                }
                200 => {
                    if offset > 0 {
                        // The server ignored the range; start over.
                        offset = 0;
                        hasher = Sha256::new();
                    }
                }
                416 => {
                    let _ = tokio::fs::remove_file(&part).await;
                    return Err(Fail::Stream("range not satisfiable".into()));
                }
                code => return Err(Fail::Http(code)),
            }

            let mut out = if offset == 0 {
                tokio::fs::File::create(&part).await
            } else {
                tokio::fs::OpenOptions::new().append(true).open(&part).await
            }
            .map_err(|e| Fail::Fatal(e.into()))?;

            loop {
                tokio::select! {
                    biased;
                    changed = rx.changed() => {
                        if changed.is_err() {
                            return Err(Fail::Cancelled);
                        }
                        let state = *rx.borrow_and_update();
                        match state {
                            RunState::Running => {}
                            RunState::Cancelled => {
                                let _ = out.flush().await;
                                return Err(Fail::Cancelled);
                            }
                            RunState::Paused => {
                                out.flush().await.map_err(|e| Fail::Fatal(e.into()))?;
                                // Dropping the response closes the connection; resuming
                                // reopens it with a Range request.
                                continue 'transfer;
                            }
                        }
                    }
                    chunk = response.chunk() => match chunk {
                        Ok(Some(bytes)) => {
                            out.write_all(&bytes).await.map_err(|e| Fail::Fatal(e.into()))?;
                            hasher.update(&bytes);
                            offset += bytes.len() as u64;
                            notify(FileEvent::Bytes(offset.min(file.size)));
                            if offset > file.size {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = out.flush().await;
                            return Err(Fail::Stream(e.to_string()));
                        }
                    }
                }
            }
            out.flush().await.map_err(|e| Fail::Fatal(e.into()))?;
            drop(out);
            if offset < file.size {
                return Err(Fail::Stream("the connection closed early".into()));
            }
        }

        if offset > file.size {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Fail::Mismatch);
        }
        notify(FileEvent::Verifying);
        let digest = hex::encode(hasher.finalize());
        if !digest.eq_ignore_ascii_case(&file.sha256) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Fail::Mismatch);
        }
        tokio::fs::rename(&part, &file.dest)
            .await
            .map_err(|e| Fail::Fatal(e.into()))?;
        Ok(())
    }
}

async fn wait_runnable(rx: &mut watch::Receiver<RunState>) -> Result<(), Fail> {
    loop {
        let state = *rx.borrow_and_update();
        match state {
            RunState::Running => return Ok(()),
            RunState::Cancelled => return Err(Fail::Cancelled),
            RunState::Paused => {
                if rx.changed().await.is_err() {
                    return Err(Fail::Cancelled);
                }
            }
        }
    }
}

fn content_range_start(value: &str) -> Option<u64> {
    value
        .strip_prefix("bytes ")?
        .split(['-', '/'])
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Feeds the first `limit` bytes of `path` to the hasher and returns how many there were, so a
/// resumed download continues the same digest.
async fn hash_prefix(path: &Path, limit: u64, hasher: &mut Sha256) -> std::io::Result<u64> {
    let path = path.to_path_buf();
    let mut taken = std::mem::take(hasher);
    let (taken, count) = tokio::task::spawn_blocking(move || -> std::io::Result<(Sha256, u64)> {
        let mut file = std::fs::File::open(path)?.take(limit);
        let mut buffer = vec![0u8; 1 << 20];
        let mut count = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            taken.update(&buffer[..read]);
            count += read as u64;
        }
        Ok((taken, count))
    })
    .await
    .map_err(std::io::Error::other)??;
    *hasher = taken;
    Ok(count)
}

fn describe(fail: &Fail) -> String {
    match fail {
        Fail::Cancelled => "cancelled".into(),
        Fail::Fatal(error) => error.to_string(),
        Fail::Connect(detail) => format!("no connection: {detail}"),
        Fail::Stream(detail) => format!("transfer broke off: {detail}"),
        Fail::Http(code) => format!("HTTP {code}"),
        Fail::Mismatch => "checksum mismatch".into(),
    }
}

fn final_error(last: Option<Fail>, saw_response: bool) -> AppError {
    match last {
        Some(Fail::Mismatch) => AppError::new(
            AppErrorCode::ChecksumMismatch,
            "The downloaded file did not match its checksum.",
        )
        .with_reason("checksum_mismatch"),
        Some(Fail::Http(code)) => AppError::new(
            AppErrorCode::DownloadFailed,
            "The server refused the download.",
        )
        .with_reason(format!("http_{code}")),
        Some(Fail::Connect(detail) | Fail::Stream(detail)) => {
            if saw_response {
                AppError::new(
                    AppErrorCode::DownloadFailed,
                    "The download was interrupted.",
                )
                .with_reason("interrupted")
                .with_detail(detail)
            } else {
                AppError::new(
                    AppErrorCode::Offline,
                    "No connection to the download server.",
                )
                .with_reason("offline")
                .with_detail(detail)
            }
        }
        Some(Fail::Fatal(error)) => error,
        Some(Fail::Cancelled) => AppError::cancelled(),
        None => AppError::new(
            AppErrorCode::DownloadFailed,
            "There is nothing to download from.",
        )
        .with_reason("no_urls"),
    }
}

/// SHA-256 of a file, reading in 1 MiB steps. `on_bytes` gets the running total. Blocking: call
/// it from a blocking task.
pub fn sha256_file(path: &Path, mut on_bytes: impl FnMut(u64)) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
        on_bytes(total);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Route, TestServer};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn file_for(dest: &Path, urls: Vec<String>, data: &[u8]) -> DownloadFile {
        DownloadFile {
            dest: dest.to_path_buf(),
            urls,
            sha256: sha256_bytes(data),
            size: data.len() as u64,
        }
    }

    fn downloader() -> Downloader {
        Downloader::new()
            .unwrap()
            .with_retry_delay(Duration::from_millis(5))
    }

    fn no_events(_: FileEvent) {}

    #[tokio::test]
    async fn downloads_verifies_and_renames() {
        let data = payload(300_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("nested").join("a.bin");
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);

        let seen = AtomicU64::new(0);
        let notify = |event: FileEvent| {
            if let FileEvent::Bytes(n) = event {
                seen.store(n, Ordering::SeqCst);
            }
        };
        downloader()
            .download(&file, &Control::new(), &notify)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert!(!part_path(&dest).exists());
        assert_eq!(seen.load(Ordering::SeqCst), data.len() as u64);
    }

    #[tokio::test]
    async fn resumes_a_partial_file_with_a_range_request() {
        let data = payload(300_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        std::fs::write(part_path(&dest), &data[..120_000]).unwrap();
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);

        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert_eq!(
            server.range_headers("/a.bin"),
            vec![Some("bytes=120000-".to_string())]
        );
    }

    #[tokio::test]
    async fn recovers_from_a_connection_that_drops_midway() {
        let data = payload(400_000);
        let route = Route::new(data.clone()).cut_first_response_at(150_000);
        let server = TestServer::start(vec![("/a.bin", route)]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);

        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
        let ranges = server.range_headers("/a.bin");
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], None);
        assert!(ranges[1].as_deref().unwrap().starts_with("bytes="));
    }

    #[tokio::test]
    async fn a_wrong_checksum_is_rejected_and_nothing_is_kept() {
        let data = payload(50_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let mut file = file_for(&dest, vec![server.url("/a.bin")], &data);
        file.sha256 = sha256_bytes(b"something else");

        let error = downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap_err();

        assert_eq!(error.code, AppErrorCode::ChecksumMismatch);
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn falls_back_to_the_next_mirror_in_order() {
        let data = payload(80_000);
        let server = TestServer::start(vec![
            ("/missing.bin", Route::status(404)),
            (
                "/broken.bin",
                Route::new(data.clone()).fail_first_requests(10),
            ),
            ("/good.bin", Route::new(data.clone())),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(
            &dest,
            vec![
                server.url("/missing.bin"),
                server.url("/broken.bin"),
                server.url("/good.bin"),
            ],
            &data,
        );

        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert_eq!(server.hits("/missing.bin"), 1);
        assert_eq!(server.hits("/broken.bin"), 1);
        assert_eq!(server.hits("/good.bin"), 1);
    }

    #[tokio::test]
    async fn a_dead_first_mirror_does_not_stop_the_second() {
        let data = payload(20_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(
            &dest,
            vec![TestServer::dead_url(), server.url("/a.bin")],
            &data,
        );
        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn unreachable_servers_are_reported_as_offline() {
        let data = payload(1000);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(
            &dest,
            vec![TestServer::dead_url(), TestServer::dead_url()],
            &data,
        );
        let error = downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::Offline);
    }

    #[tokio::test]
    async fn http_errors_everywhere_are_a_download_failure() {
        let data = payload(1000);
        let server = TestServer::start(vec![("/a.bin", Route::status(404))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);
        let error = downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::DownloadFailed);
        assert_eq!(error.reason.as_deref(), Some("http_404"));
    }

    #[tokio::test]
    async fn a_server_that_ignores_range_restarts_the_file() {
        let data = payload(200_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()).ignore_range())]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        std::fs::write(part_path(&dest), &data[..50_000]).unwrap();
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);
        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn a_finished_file_is_not_downloaded_again() {
        let data = payload(10_000);
        let server = TestServer::start(vec![("/a.bin", Route::new(data.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        std::fs::write(&dest, &data).unwrap();
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);
        downloader()
            .download(&file, &Control::new(), &no_events)
            .await
            .unwrap();
        assert_eq!(server.hits("/a.bin"), 0);
    }

    #[tokio::test]
    async fn pause_stops_the_transfer_and_resume_continues_it() {
        let data = payload(600_000);
        let route = Route::new(data.clone()).slow(Duration::from_millis(15));
        let server = TestServer::start(vec![("/a.bin", route)]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);
        let control = Control::new();
        let seen = Arc::new(AtomicU64::new(0));

        let task = {
            let control = control.clone();
            let seen = seen.clone();
            let dest = dest.clone();
            tokio::spawn(async move {
                let notify = move |event: FileEvent| {
                    if let FileEvent::Bytes(n) = event {
                        seen.store(n, Ordering::SeqCst);
                    }
                };
                let file = DownloadFile { dest, ..file };
                downloader().download(&file, &control, &notify).await
            })
        };

        while seen.load(Ordering::SeqCst) < 20_000 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        control.pause();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let paused_at = seen.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            seen.load(Ordering::SeqCst),
            paused_at,
            "no bytes while paused"
        );
        assert!(paused_at < data.len() as u64);
        assert!(part_path(&dest).exists());
        assert!(!task.is_finished());

        control.resume();
        task.await.unwrap().unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        let ranges = server.range_headers("/a.bin");
        assert!(ranges.len() >= 2 && ranges.last().unwrap().is_some());
    }

    #[tokio::test]
    async fn cancel_keeps_the_partial_file_and_reports_cancelled() {
        let data = payload(600_000);
        let route = Route::new(data.clone()).slow(Duration::from_millis(15));
        let server = TestServer::start(vec![("/a.bin", route)]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let file = file_for(&dest, vec![server.url("/a.bin")], &data);
        let control = Control::new();
        let seen = Arc::new(AtomicU64::new(0));
        let task = {
            let control = control.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let notify = move |event: FileEvent| {
                    if let FileEvent::Bytes(n) = event {
                        seen.store(n, Ordering::SeqCst);
                    }
                };
                downloader().download(&file, &control, &notify).await
            })
        };
        while seen.load(Ordering::SeqCst) < 20_000 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        control.cancel();
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code, AppErrorCode::Cancelled);
        assert!(part_path(&dest).exists());
        assert!(!dest.exists());
    }

    #[test]
    fn content_range_is_parsed() {
        assert_eq!(content_range_start("bytes 100-199/200"), Some(100));
        assert_eq!(content_range_start("bytes */200"), None);
        assert_eq!(content_range_start("items 1-2/3"), None);
    }

    #[test]
    fn sha256_file_matches_the_bytes_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(sha256_file(&path, |_| {}).unwrap(), sha256_bytes(b"hello"));
    }
}
