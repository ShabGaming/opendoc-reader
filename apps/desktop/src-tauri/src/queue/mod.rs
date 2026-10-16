//! The job queue (M1-10): files in, converted files out, one at a time.
//!
//! The Rust core owns the queue; the engine only ever sees one page per call. State lives in
//! SQLite ([`db`]) so it survives a restart, and a file is resumed from its first page without
//! an IR file in `<work>/<fileId>/ir/`. Two background tasks run it: the inspector looks at newly
//! added files (page count, text layer, preview), and the runner converts them in order.

pub mod db;
mod export_to;
pub mod naming;
pub mod state;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

use crate::contract::{
    AppError, AppErrorCode, AttentionReason, ConvertOptions, EngineInfo, ExportFormat, FileKind,
    OutputFile, PageState, PageStatus, QueueItem, QueueProgress, QueueStatus, ReviewPage,
    SaveLocation, Stage, TextMode, EVENT_QUEUE_PROGRESS,
};
use crate::error::Result;
use crate::manifest::ModelEntry;
use crate::paths::path_str;
use crate::services::{Services, Throttle};
use crate::supervisor::{EngineError, Progress, ProgressFn};
use db::{Db, FileRow};

const EVENT_INTERVAL: Duration = Duration::from_millis(100);
/// Engine error code of BAD_INPUT: the file itself is the problem.
const BAD_INPUT: i64 = -32004;
const ALREADY_EXISTS: &str = "already_exists";

#[derive(Default)]
struct Live {
    current_page: Option<u32>,
    stage: Option<Stage>,
    seconds_left: Option<f64>,
}

#[derive(Default)]
struct Inner {
    run_enabled: bool,
    paused: bool,
    last_options: Option<ConvertOptions>,
    current: Option<String>,
    cancel_requested: HashSet<String>,
    live: HashMap<String, Live>,
}

enum Outcome {
    Done,
    Paused,
    NeedsAttention,
}

struct Inspected {
    page_count: u32,
    has_text_layer: bool,
    preview: Option<String>,
}

pub struct Queue {
    svc: Arc<Services>,
    db: Db,
    inner: Mutex<Inner>,
    wake: Notify,
    inspect_tx: mpsc::UnboundedSender<String>,
    inspect_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    throttle: Throttle,
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// True for an IR page file that exists and parses (the resume check of docs/protocol.md 1.3).
fn ir_is_valid(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|value| value.get("blocks").is_some())
}

fn engine_info_of(value: &Value) -> Option<EngineInfo> {
    serde_json::from_value(value.get("engine")?.clone()).ok()
}

fn stage_of(name: &str) -> Option<Stage> {
    Some(match name {
        "prepare" => Stage::Prepare,
        "layout" => Stage::Layout,
        "recognize" => Stage::Recognize,
        "normalize" => Stage::Normalize,
        "crops" => Stage::Crops,
        _ => return None,
    })
}

fn kind_name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Pdf => "pdf",
        FileKind::Image => "image",
    }
}

impl Queue {
    pub fn new(svc: Arc<Services>) -> Result<Arc<Self>> {
        let db = Db::open(&svc.paths.database)?;
        Self::with_db(svc, db)
    }

    pub fn with_db(svc: Arc<Services>, db: Db) -> Result<Arc<Self>> {
        let (inspect_tx, inspect_rx) = mpsc::unbounded_channel();
        let queue = Arc::new(Self {
            svc,
            db,
            inner: Mutex::new(Inner::default()),
            wake: Notify::new(),
            inspect_tx,
            inspect_rx: Mutex::new(Some(inspect_rx)),
            throttle: Throttle::new(EVENT_INTERVAL),
        });
        queue.recover()?;
        Ok(queue)
    }

    /// After a restart nothing is converting: interrupted items go back to waiting, and the
    /// finished pages of each file are counted again from disk.
    fn recover(&self) -> Result<()> {
        for row in self.db.all()? {
            let status = state::after_restart(row.status);
            let done = match (status, row.page_count) {
                (QueueStatus::Done, Some(count)) => count,
                (_, Some(count)) => self.count_pages(&row.file_id, count),
                _ => 0,
            };
            if status != row.status || done != row.pages_done {
                self.db.update(&row.file_id, |r| {
                    r.status = status;
                    r.pages_done = done;
                })?;
            }
        }
        Ok(())
    }

    fn count_pages(&self, file_id: &str, page_count: u32) -> u32 {
        (0..page_count)
            .filter(|page| ir_is_valid(&self.svc.paths.page_ir(file_id, *page)))
            .count() as u32
    }

    // -----------------------------------------------------------------------------------------
    // Items and events

    fn item_of(&self, row: &FileRow) -> QueueItem {
        let inner = self.inner.lock().expect("queue lock");
        let live = inner
            .live
            .get(&row.file_id)
            .filter(|_| row.status == QueueStatus::Converting);
        QueueItem {
            file_id: row.file_id.clone(),
            path: row.path.clone(),
            name: row.name.clone(),
            kind: row.kind,
            size_bytes: row.size_bytes,
            page_count: row.page_count,
            has_text_layer: row.has_text_layer,
            preview_path: row.preview_path.clone(),
            status: row.status,
            attention: if row.status == QueueStatus::NeedsAttention {
                row.attention
            } else {
                None
            },
            text_mode: row.text_mode,
            model_id: row.model_id.clone(),
            formats: row.formats.clone(),
            pages_done: row.pages_done,
            current_page: live.and_then(|l| l.current_page),
            stage: live.and_then(|l| l.stage),
            seconds_left: live.and_then(|l| l.seconds_left),
            duration_ms: row.duration_ms,
            outputs: row.outputs.clone(),
            error: row.error.clone(),
        }
    }

    fn emit_row(&self, row: &FileRow, page: Option<PageStatus>, important: bool) {
        if !self.throttle.allow(&row.file_id, important) {
            return;
        }
        let payload = QueueProgress {
            item: self.item_of(row),
            page,
        };
        if let Ok(value) = serde_json::to_value(&payload) {
            self.svc.sink.emit(EVENT_QUEUE_PROGRESS, value);
        }
    }

    fn emit(&self, file_id: &str, page: Option<PageStatus>, important: bool) {
        if let Ok(Some(row)) = self.db.get(file_id) {
            self.emit_row(&row, page, important);
        }
    }

    fn set_live(&self, file_id: &str, change: impl FnOnce(&mut Live)) {
        let mut inner = self.inner.lock().expect("queue lock");
        change(inner.live.entry(file_id.to_string()).or_default());
    }

    fn row(&self, file_id: &str) -> Result<FileRow> {
        self.db.get(file_id)?.ok_or_else(|| {
            AppError::not_found("No such file in the queue.").with_reason("no_such_file")
        })
    }

    fn set_status(
        &self,
        file_id: &str,
        status: QueueStatus,
        error: Option<AppError>,
    ) -> Result<()> {
        self.db.update(file_id, |row| {
            row.status = status;
            row.error = error;
            if status != QueueStatus::NeedsAttention {
                row.attention = None;
            }
        })?;
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Commands

    pub fn add(&self, paths: Vec<String>) -> Result<Vec<QueueItem>> {
        let files = naming::collect(&paths);
        if files.is_empty() {
            return Err(AppError::new(
                AppErrorCode::BadInput,
                "None of these files can be converted.",
            )
            .with_reason("unsupported_format"));
        }
        // Compared as paths, so "a/b.pdf" and "a\b.pdf" count as the same file on Windows.
        let active: Vec<PathBuf> = self
            .db
            .all()?
            .into_iter()
            .filter(|row| state::is_active(row.status))
            .map(|row| PathBuf::from(row.path))
            .collect();
        let settings = self.svc.settings.get();
        let mut added = Vec::new();
        for file in files {
            let path = path_str(&file);
            if active.contains(&file)
                || added.iter().any(|i: &QueueItem| Path::new(&i.path) == file)
            {
                continue;
            }
            let mut row = FileRow {
                file_id: self.new_file_id()?,
                seq: 0,
                path,
                name: file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                kind: naming::kind_of(&file),
                size_bytes: std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
                page_count: None,
                has_text_layer: None,
                preview_path: None,
                status: QueueStatus::Waiting,
                attention: None,
                text_mode: None,
                model_id: None,
                formats: settings.default_formats.clone(),
                save_to: settings.save_to.clone(),
                pages_done: 0,
                duration_ms: None,
                outputs: Vec::new(),
                error: None,
                engine: None,
                added_at: now_seconds(),
            };
            self.db.insert(&mut row)?;
            let item = self.item_of(&row);
            self.emit_row(&row, None, true);
            let _ = self.inspect_tx.send(row.file_id.clone());
            added.push(item);
        }
        Ok(added)
    }

    fn new_file_id(&self) -> Result<String> {
        loop {
            let id = format!("f-{:08x}", rand::random::<u32>());
            if self.db.get(&id)?.is_none() {
                return Ok(id);
            }
        }
    }

    pub fn list(&self) -> Result<Vec<QueueItem>> {
        Ok(self.db.all()?.iter().map(|row| self.item_of(row)).collect())
    }

    /// `queue_start`: stamps the options on every waiting item and lets the runner go.
    pub fn start(&self, options: ConvertOptions) -> Result<()> {
        if !self.svc.model_installed(&options.model_id) {
            self.svc.model(&options.model_id)?;
            return Err(
                AppError::new(AppErrorCode::ModelNotReady, "The model is not installed.")
                    .with_reason("files_missing")
                    .with_detail(options.model_id.clone()),
            );
        }
        let formats = if options.formats.is_empty() {
            self.svc.settings.get().default_formats
        } else {
            options.formats.clone()
        };
        for row in self.db.all()? {
            if state::can_start(row.status) {
                self.db.update(&row.file_id, |r| {
                    r.model_id = Some(options.model_id.clone());
                    r.formats = formats.clone();
                    r.save_to = options.save_to.clone();
                })?;
                self.emit(&row.file_id, None, true);
            }
        }
        {
            let mut inner = self.inner.lock().expect("queue lock");
            inner.run_enabled = true;
            inner.paused = false;
            inner.last_options = Some(ConvertOptions { formats, ..options });
        }
        self.wake.notify_one();
        Ok(())
    }

    pub fn pause(&self) {
        self.inner.lock().expect("queue lock").paused = true;
    }

    pub fn resume(&self) {
        self.inner.lock().expect("queue lock").paused = false;
        self.wake.notify_one();
    }

    pub async fn cancel(&self, file_id: Option<String>) -> Result<()> {
        let single = file_id.is_some();
        let rows = match file_id {
            Some(id) => vec![self.row(&id)?],
            None => self.db.all()?,
        };
        let mut stop_engine = false;
        for row in rows {
            if !state::can_cancel(row.status) {
                if single {
                    return Err(AppError::invalid("This file cannot be cancelled now.")
                        .with_reason("not_cancellable"));
                }
                continue;
            }
            if row.status == QueueStatus::Converting {
                self.inner
                    .lock()
                    .expect("queue lock")
                    .cancel_requested
                    .insert(row.file_id.clone());
                stop_engine = true;
            } else {
                self.set_status(&row.file_id, QueueStatus::Cancelled, None)?;
                self.emit(&row.file_id, None, true);
            }
        }
        if stop_engine {
            self.svc.supervisor.cancel_current().await;
        }
        Ok(())
    }

    pub fn remove(&self, file_id: &str) -> Result<()> {
        let row = self.row(file_id)?;
        if !state::can_remove(row.status) {
            return Err(
                AppError::invalid("A file that is converting cannot be removed.")
                    .with_reason("converting"),
            );
        }
        self.forget(file_id)
    }

    fn forget(&self, file_id: &str) -> Result<()> {
        self.db.delete(file_id)?;
        self.inner.lock().expect("queue lock").live.remove(file_id);
        self.throttle.forget(file_id);
        let dir = self.svc.paths.job_dir(file_id);
        if dir.exists() {
            if let Err(error) = std::fs::remove_dir_all(&dir) {
                tracing::warn!("could not remove a job folder: {error}");
            }
        }
        Ok(())
    }

    pub fn clear_done(&self) -> Result<Vec<String>> {
        let mut removed = Vec::new();
        for row in self.db.all()? {
            if state::is_finished(row.status) {
                self.forget(&row.file_id)?;
                removed.push(row.file_id);
            }
        }
        Ok(removed)
    }

    pub fn retry(&self, file_id: &str) -> Result<()> {
        let row = self.row(file_id)?;
        if !state::can_retry(row.status) {
            return Err(
                AppError::invalid("Only a failed or cancelled file can be retried.")
                    .with_reason("not_retryable"),
            );
        }
        let last = self.inner.lock().expect("queue lock").last_options.clone();
        let settings = self.svc.settings.get();
        let model_id = row
            .model_id
            .clone()
            .or_else(|| last.as_ref().map(|o| o.model_id.clone()))
            .or(settings.default_model_id)
            .ok_or_else(|| {
                AppError::invalid("There are no options to retry with.").with_reason("no_options")
            })?;
        let formats = if row.formats.is_empty() {
            last.as_ref()
                .map(|o| o.formats.clone())
                .unwrap_or(settings.default_formats)
        } else {
            row.formats.clone()
        };
        let save_to = last
            .as_ref()
            .filter(|_| row.model_id.is_none())
            .map(|o| o.save_to.clone())
            .unwrap_or_else(|| row.save_to.clone());
        let done = row
            .page_count
            .map(|n| self.count_pages(file_id, n))
            .unwrap_or(0);
        self.db.clear_page_errors(file_id)?;
        self.db.update(file_id, |r| {
            r.status = QueueStatus::Waiting;
            r.error = None;
            r.model_id = Some(model_id);
            r.formats = formats;
            r.save_to = save_to;
            r.pages_done = done;
        })?;
        self.emit(file_id, None, true);
        {
            let mut inner = self.inner.lock().expect("queue lock");
            inner.run_enabled = true;
            inner.cancel_requested.remove(file_id);
        }
        self.wake.notify_one();
        Ok(())
    }

    pub fn set_text_mode(&self, file_id: &str, mode: TextMode) -> Result<()> {
        let row = self.row(file_id)?;
        if !state::can_set_text_mode(row.status) {
            return Err(
                AppError::invalid("This file is not waiting for a text decision.")
                    .with_reason("not_waiting_for_text_mode"),
            );
        }
        let (run_enabled, last) = {
            let inner = self.inner.lock().expect("queue lock");
            (inner.run_enabled, inner.last_options.clone())
        };
        self.db.update(file_id, |r| {
            r.text_mode = Some(mode);
            r.status = QueueStatus::Waiting;
            r.attention = None;
            if let (true, Some(options)) = (run_enabled, &last) {
                r.model_id = Some(options.model_id.clone());
                r.formats = options.formats.clone();
                r.save_to = options.save_to.clone();
            }
        })?;
        self.emit(file_id, None, true);
        self.wake.notify_one();
        Ok(())
    }

    /// Models that a conversion is using right now (they cannot be removed).
    pub fn models_in_use(&self) -> Vec<String> {
        self.db
            .all()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.status == QueueStatus::Converting)
            .filter_map(|row| row.model_id)
            .collect()
    }

    pub fn pages(&self, file_id: &str) -> Result<Vec<PageStatus>> {
        let row = self.row(file_id)?;
        let errors = self.db.page_errors(file_id)?;
        let current = {
            let inner = self.inner.lock().expect("queue lock");
            inner
                .live
                .get(file_id)
                .filter(|_| row.status == QueueStatus::Converting)
                .and_then(|l| l.current_page)
        };
        let count = row.page_count.unwrap_or(0);
        Ok((0..count)
            .map(|index| {
                let image = self.svc.paths.page_image(file_id, index);
                let (state, error) = if ir_is_valid(&self.svc.paths.page_ir(file_id, index)) {
                    (PageState::Done, None)
                } else if let Some(error) = errors.get(&index) {
                    (PageState::Failed, Some(error.clone()))
                } else if current == Some(index) {
                    (PageState::Converting, None)
                } else {
                    (PageState::Pending, None)
                };
                PageStatus {
                    index,
                    state,
                    image_path: image.is_file().then(|| path_str(&image)),
                    error,
                }
            })
            .collect())
    }

    pub async fn review(&self, file_id: &str, page: u32) -> Result<ReviewPage> {
        let row = self.row(file_id)?;
        let ir_path = self.svc.paths.page_ir(file_id, page);
        let bytes = std::fs::read(&ir_path).map_err(|_| {
            AppError::not_found("This page has no result yet.").with_reason("no_result")
        })?;
        let value: Value = serde_json::from_slice(&bytes)?;
        let image = value
            .get("sourceImage")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| path_str(&self.svc.paths.page_image(file_id, page)));
        let engine = self.engine_info_for(&row).await;
        Ok(ReviewPage {
            file_id: file_id.to_string(),
            page_index: page,
            page_count: row.page_count.unwrap_or(0),
            image_path: image,
            page: value,
            engine,
        })
    }

    /// The engine that produced the file's pages, from the last page result; reconstructed from
    /// the model when the file was finished before it was recorded.
    async fn engine_info_for(&self, row: &FileRow) -> EngineInfo {
        if let Some(info) = &row.engine {
            return info.clone();
        }
        let version = self
            .svc
            .supervisor
            .engine_version()
            .await
            .unwrap_or_else(|| "0.1.0".to_string());
        if row.text_mode == Some(TextMode::ExistingText) {
            return EngineInfo {
                id: "pdf-text".into(),
                model: "text-layer".into(),
                version,
            };
        }
        let model_id = row.model_id.clone().unwrap_or_default();
        let engine = self
            .svc
            .manifest
            .model(&model_id)
            .map(|m| m.engine.clone())
            .unwrap_or_default();
        EngineInfo {
            id: engine,
            model: model_id,
            version,
        }
    }

    pub async fn export(&self, file_id: &str, format: ExportFormat) -> Result<OutputFile> {
        let row = self.row(file_id)?;
        if row.status != QueueStatus::Done {
            return Err(
                AppError::invalid("Only a finished file can be exported.").with_reason("not_done")
            );
        }
        let document = self.ensure_document(&row).await?;
        let outputs = self.write_outputs(&document, &row, &[format]).await?;
        let output = outputs
            .into_iter()
            .next()
            .ok_or_else(|| AppError::internal("The export wrote nothing."))?;
        let added = output.clone();
        self.db.update(file_id, |r| r.outputs.push(added))?;
        self.emit(file_id, None, true);
        Ok(output)
    }

    // -----------------------------------------------------------------------------------------
    // Inspection

    /// Queues every file that has not been inspected yet (after startup, and once the engine
    /// has been installed).
    pub fn inspect_pending(&self) {
        for row in self.db.all().unwrap_or_default() {
            if row.status == QueueStatus::Waiting && row.page_count.is_none() {
                let _ = self.inspect_tx.send(row.file_id);
            }
        }
    }

    pub async fn inspect_loop(self: Arc<Self>) {
        let Some(mut receiver) = self.inspect_rx.lock().expect("queue lock").take() else {
            return;
        };
        while let Some(file_id) = receiver.recv().await {
            self.inspect_new(&file_id).await;
        }
    }

    async fn inspect_new(&self, file_id: &str) {
        let Ok(Some(row)) = self.db.get(file_id) else {
            return;
        };
        let busy = self.inner.lock().expect("queue lock").current.as_deref() == Some(file_id);
        if row.status != QueueStatus::Waiting || busy {
            return;
        }
        match self.inspect_row(&row).await {
            Ok(found) => {
                let updated = self.db.update(file_id, |r| {
                    if r.status == QueueStatus::Waiting {
                        apply_inspection(r, &found);
                    }
                });
                if let Ok(Some(row)) = updated {
                    self.emit_row(&row, None, true);
                }
            }
            Err(error) if error.code() == Some(BAD_INPUT) => {
                let app = error.to_app_error();
                let _ = self.set_status(file_id, QueueStatus::Failed, Some(app));
                self.emit(file_id, None, true);
            }
            Err(error) => {
                // Usually the engine is not installed yet; it is tried again when conversion
                // starts and after setup.
                tracing::debug!("could not inspect a file yet: {error}");
            }
        }
    }

    /// Asks the engine about the file and renders page 0 as the preview.
    async fn inspect_row(&self, row: &FileRow) -> std::result::Result<Inspected, EngineError> {
        let method = match row.kind {
            FileKind::Pdf => "pdf.inspect",
            FileKind::Image => "image.inspect",
        };
        let found = self
            .svc
            .supervisor
            .call(method, json!({"path": row.path}), None)
            .await?;
        let pages = found
            .get("pages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let page_count = found
            .get("pageCount")
            .and_then(Value::as_u64)
            .unwrap_or(pages.len() as u64) as u32;
        let with_text = pages
            .iter()
            .filter(|p| p.get("hasTextLayer").and_then(Value::as_bool) == Some(true))
            .count() as u32;
        let has_text_layer =
            row.kind == FileKind::Pdf && page_count > 0 && with_text * 2 >= page_count;

        let mut preview = None;
        if page_count > 0 {
            let out = self.svc.paths.page_image(&row.file_id, 0);
            let rendered = self
                .svc
                .supervisor
                .call(
                    "page.render",
                    json!({"source": row.path, "pageIndex": 0, "outPath": path_str(&out)}),
                    None,
                )
                .await;
            match rendered {
                Ok(_) => preview = Some(path_str(&out)),
                Err(error) => tracing::warn!("could not render a preview: {error}"),
            }
        }
        Ok(Inspected {
            page_count,
            has_text_layer,
            preview,
        })
    }

    // -----------------------------------------------------------------------------------------
    // The runner

    pub async fn run_loop(self: Arc<Self>) {
        loop {
            self.wake.notified().await;
            while let Some(file_id) = self.next_runnable() {
                self.run_item(&file_id).await;
            }
        }
    }

    fn next_runnable(&self) -> Option<String> {
        {
            let inner = self.inner.lock().expect("queue lock");
            if !inner.run_enabled || inner.paused {
                return None;
            }
        }
        self.db
            .all()
            .ok()?
            .into_iter()
            .filter(|row| state::is_runnable(row.status, row.model_id.is_some()))
            .min_by_key(|row| (state::pick_order(row.status), row.seq))
            .map(|row| row.file_id)
    }

    async fn run_item(self: &Arc<Self>, file_id: &str) {
        let started = Instant::now();
        {
            let mut inner = self.inner.lock().expect("queue lock");
            inner.current = Some(file_id.to_string());
            inner.cancel_requested.remove(file_id);
            inner.live.insert(file_id.to_string(), Live::default());
        }
        let _ = self.set_status(file_id, QueueStatus::Converting, None);
        self.emit(file_id, None, true);

        let outcome = self.convert(file_id).await;

        let status_and_error = match outcome {
            Ok(Outcome::Done) => (QueueStatus::Done, None),
            Ok(Outcome::Paused) => (QueueStatus::Paused, None),
            Ok(Outcome::NeedsAttention) => (QueueStatus::NeedsAttention, None),
            Err(error) if error.code == AppErrorCode::Cancelled => (QueueStatus::Cancelled, None),
            Err(error) => {
                tracing::warn!("a conversion failed: {error}");
                (QueueStatus::Failed, Some(error))
            }
        };
        {
            let mut inner = self.inner.lock().expect("queue lock");
            inner.current = None;
            inner.cancel_requested.remove(file_id);
            inner.live.remove(file_id);
        }
        let elapsed = started.elapsed().as_millis() as u64;
        let (status, error) = status_and_error;
        let updated = self.db.update(file_id, |row| {
            row.status = status;
            row.error = error;
            if status == QueueStatus::NeedsAttention {
                row.attention = Some(AttentionReason::HasTextLayer);
            }
            if status == QueueStatus::Done {
                row.duration_ms = Some(elapsed);
            }
        });
        if let Ok(Some(row)) = updated {
            self.emit_row(&row, None, true);
        }
    }

    fn cancelled(&self, file_id: &str) -> bool {
        self.inner
            .lock()
            .expect("queue lock")
            .cancel_requested
            .contains(file_id)
    }

    fn check_cancel(&self, file_id: &str) -> Result<()> {
        if self.cancelled(file_id) {
            Err(AppError::cancelled())
        } else {
            Ok(())
        }
    }

    fn pause_requested(&self) -> bool {
        self.inner.lock().expect("queue lock").paused
    }

    fn set_stage(&self, file_id: &str, page: Option<u32>, stage: Stage) {
        self.set_live(file_id, |live| {
            if let Some(page) = page {
                live.current_page = Some(page);
            }
            live.stage = Some(stage);
        });
    }

    async fn convert(self: &Arc<Self>, file_id: &str) -> Result<Outcome> {
        let mut row = self.row(file_id)?;
        let model_id = row
            .model_id
            .clone()
            .ok_or_else(|| AppError::invalid("No model was chosen."))?;
        let model = self.svc.model(&model_id)?.clone();

        self.set_stage(file_id, None, Stage::Prepare);
        self.emit(file_id, None, false);
        self.svc.prepare_model(&model_id).await?;
        self.check_cancel(file_id)?;

        if row.page_count.is_none() {
            let found = self.inspect_row(&row).await.map_err(|e| e.to_app_error())?;
            if found.page_count == 0 {
                return Err(
                    AppError::new(AppErrorCode::BadInput, "The file has no pages.")
                        .with_reason("corrupt"),
                );
            }
            let needs_question = found.has_text_layer && row.text_mode.is_none();
            let updated = self
                .db
                .update(file_id, |r| apply_inspection_keep_status(r, &found))?;
            row = updated.ok_or_else(|| AppError::not_found("The file was removed."))?;
            if needs_question {
                return Ok(Outcome::NeedsAttention);
            }
        }
        let page_count = row.page_count.unwrap_or(0);
        let mode = row.text_mode.unwrap_or(TextMode::Ocr);
        self.db.update(file_id, |r| r.text_mode = Some(mode))?;

        let finished = self.count_pages(file_id, page_count);
        self.db.update(file_id, |r| r.pages_done = finished)?;
        let mut durations: Vec<f64> = Vec::new();
        for page in 0..page_count {
            self.check_cancel(file_id)?;
            if self.pause_requested() {
                return Ok(Outcome::Paused);
            }
            if ir_is_valid(&self.svc.paths.page_ir(file_id, page)) {
                continue;
            }
            let began = Instant::now();
            self.set_live(file_id, |l| {
                l.current_page = Some(page);
                l.stage = Some(Stage::Render);
            });
            self.emit_page(file_id, page, PageState::Converting, None, false);

            match self
                .convert_page(file_id, &row, &model, &model_id, mode, page)
                .await
            {
                Ok(info) => {
                    durations.push(began.elapsed().as_secs_f64());
                    let done = self.count_pages(file_id, page_count);
                    self.db.clear_page_error(file_id, page)?;
                    self.db.update(file_id, |r| {
                        r.pages_done = done;
                        r.engine = Some(info);
                    })?;
                    let average = durations.iter().sum::<f64>() / durations.len() as f64;
                    self.set_live(file_id, |l| {
                        l.seconds_left = Some(average * f64::from(page_count - done));
                    });
                    self.emit_page(file_id, page, PageState::Done, None, true);
                }
                Err(error) => {
                    if error.code != AppErrorCode::Cancelled {
                        self.db.set_page_error(file_id, page, &error)?;
                        self.emit_page(file_id, page, PageState::Failed, Some(error.clone()), true);
                    }
                    return Err(error);
                }
            }
        }

        self.check_cancel(file_id)?;
        self.set_live(file_id, |l| {
            l.stage = Some(Stage::Export);
            l.seconds_left = None;
        });
        self.emit(file_id, None, false);
        let fresh = self.row(file_id)?;
        let document = self.assemble(&fresh, page_count).await?;
        let outputs = self
            .write_outputs(&document, &fresh, &fresh.formats.clone())
            .await?;
        self.db.update(file_id, |r| r.outputs = outputs)?;
        Ok(Outcome::Done)
    }

    fn emit_page(
        &self,
        file_id: &str,
        index: u32,
        state: PageState,
        error: Option<AppError>,
        important: bool,
    ) {
        let image = self.svc.paths.page_image(file_id, index);
        let page = PageStatus {
            index,
            state,
            image_path: image.is_file().then(|| path_str(&image)),
            error,
        };
        self.emit(file_id, Some(page), important);
    }

    async fn convert_page(
        self: &Arc<Self>,
        file_id: &str,
        row: &FileRow,
        model: &ModelEntry,
        model_id: &str,
        mode: TextMode,
        page: u32,
    ) -> Result<EngineInfo> {
        let paths = &self.svc.paths;
        let image = paths.page_image(file_id, page);
        let ir = paths.page_ir(file_id, page);

        if mode == TextMode::ExistingText || !image.is_file() {
            let mut params = json!({
                "source": row.path,
                "pageIndex": page,
                "outPath": path_str(&image),
            });
            if mode == TextMode::ExistingText {
                // text.extract maps PDF coordinates to pixels by a plain scale.
                params["rotate"] = json!(0);
                params["deskew"] = json!(false);
            }
            self.svc
                .supervisor
                .call("page.render", params, None)
                .await
                .map_err(|e| e.to_app_error())?;
        }
        self.check_cancel(file_id)?;

        let progress: ProgressFn = {
            let this = self.clone();
            let id = file_id.to_string();
            Arc::new(move |p: Progress| {
                if let Some(stage) = stage_of(&p.stage) {
                    this.set_stage(&id, None, stage);
                    this.emit(&id, None, false);
                }
            })
        };
        self.set_stage(file_id, Some(page), Stage::Prepare);
        let result = match mode {
            TextMode::Ocr => {
                self.svc
                    .supervisor
                    .call(
                        "convert.page",
                        json!({
                            "engineId": model.engine,
                            "modelId": model_id,
                            "imagePath": path_str(&image),
                            "pageIndex": page,
                            "irPath": path_str(&ir),
                            "cropsDir": path_str(&paths.crops_dir(file_id)),
                            "options": {},
                        }),
                        Some(progress),
                    )
                    .await
            }
            TextMode::ExistingText => {
                self.svc
                    .supervisor
                    .call(
                        "text.extract",
                        json!({
                            "source": row.path,
                            "pageIndex": page,
                            "imagePath": path_str(&image),
                            "irPath": path_str(&ir),
                        }),
                        None,
                    )
                    .await
            }
        }
        .map_err(|e| e.to_app_error())?;
        engine_info_of(&result)
            .ok_or_else(|| AppError::internal("The engine's answer has no engine info."))
    }

    /// Joins the page files into `document.json` (`document.assemble`).
    async fn assemble(&self, row: &FileRow, page_count: u32) -> Result<PathBuf> {
        let out = self.svc.paths.document_json(&row.file_id);
        let pages: Vec<String> = (0..page_count)
            .map(|p| path_str(&self.svc.paths.page_ir(&row.file_id, p)))
            .collect();
        let engine = self.engine_info_for(row).await;
        self.svc
            .supervisor
            .call(
                "document.assemble",
                json!({
                    "source": {
                        "path": row.path,
                        "kind": kind_name(row.kind),
                        "pageCount": page_count,
                    },
                    "engine": engine,
                    "pages": pages,
                    "outPath": path_str(&out),
                }),
                None,
            )
            .await
            .map_err(|e| e.to_app_error())?;
        Ok(out)
    }

    async fn ensure_document(&self, row: &FileRow) -> Result<PathBuf> {
        let existing = self.svc.paths.document_json(&row.file_id);
        if existing.is_file() {
            return Ok(existing);
        }
        let count = row.page_count.unwrap_or(0);
        if self.count_pages(&row.file_id, count) != count {
            return Err(AppError::not_found("The page results are gone.").with_reason("no_result"));
        }
        self.assemble(row, count).await
    }

    /// Writes the exports under names that are free, retrying with the next number when the
    /// engine finds the name taken after all (another export won the race).
    async fn write_outputs(
        &self,
        document: &Path,
        row: &FileRow,
        formats: &[ExportFormat],
    ) -> Result<Vec<OutputFile>> {
        let source = PathBuf::from(&row.path);
        let dir = match &row.save_to {
            SaveLocation::Original => source
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            SaveLocation::Folder { path } => PathBuf::from(path),
        };
        std::fs::create_dir_all(&dir)?;
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_string());

        let mut attempts = 0;
        loop {
            attempts += 1;
            let plan = naming::plan_outputs(&dir, &stem, formats);
            match self.run_exports(document, &plan).await {
                Ok(outputs) => return Ok(outputs),
                Err(error) if error.reason.as_deref() == Some(ALREADY_EXISTS) && attempts < 5 => {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn run_exports(
        &self,
        document: &Path,
        plan: &naming::OutputPlan,
    ) -> Result<Vec<OutputFile>> {
        let mut outputs = Vec::new();
        if let Some(docx) = &plan.docx {
            self.svc
                .supervisor
                .call(
                    "export.docx",
                    json!({"document": path_str(document), "outputPath": path_str(docx)}),
                    None,
                )
                .await
                .map_err(|e| e.to_app_error())?;
            outputs.push(OutputFile {
                format: ExportFormat::Docx,
                path: path_str(docx),
                assets_dir: None,
            });
        }
        if let Some((markdown, assets)) = &plan.markdown {
            self.svc
                .supervisor
                .call(
                    "export.markdown",
                    json!({
                        "document": path_str(document),
                        "outputPath": path_str(markdown),
                        "assetsDir": path_str(assets),
                    }),
                    None,
                )
                .await
                .map_err(|e| e.to_app_error())?;
            outputs.push(OutputFile {
                format: ExportFormat::Markdown,
                path: path_str(markdown),
                assets_dir: assets.is_dir().then(|| path_str(assets)),
            });
        }
        Ok(outputs)
    }
}

/// Records what inspection found and asks the text question for a PDF that has a text layer.
fn apply_inspection(row: &mut FileRow, found: &Inspected) {
    apply_inspection_keep_status(row, found);
    if found.has_text_layer && row.text_mode.is_none() {
        row.status = QueueStatus::NeedsAttention;
        row.attention = Some(AttentionReason::HasTextLayer);
    }
}

fn apply_inspection_keep_status(row: &mut FileRow, found: &Inspected) {
    row.page_count = Some(found.page_count);
    row.has_text_layer = Some(found.has_text_layer);
    if found.preview.is_some() {
        row.preview_path = found.preview.clone();
    }
}

#[cfg(test)]
mod tests;
