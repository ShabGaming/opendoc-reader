//! First-run setup with real downloads, limited to small files: the llama.cpp CPU build
//! (about 20 MB) and the Fast tier (about 175 MB) from the URLs in `models/manifest.json`.
//!
//! ```text
//! OPENDOC_INTEGRATION=1 OPENDOC_E2E_DOWNLOAD=1 cargo test --test e2e_download -- --nocapture
//! ```
//!
//! It needs network access to GitHub, Hugging Face and the Python package index, and uses a
//! fresh temporary app-data folder (`OPENDOC_E2E_DATA` keeps one instead).

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{enabled, engine_source, uv};
use opendoc_reader_lib::app_core::Core;
use opendoc_reader_lib::contract::{
    ConvertOptions, ExportFormat, QueueStatus, SaveLocation, SetupPhase,
};
use opendoc_reader_lib::download::{Control, DownloadFile};
use opendoc_reader_lib::paths::AppPaths;
use opendoc_reader_lib::runtime;
use opendoc_reader_lib::services::EventSink;
use serde_json::Value;

struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: &str, _payload: Value) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_downloads_the_cpu_runtime_and_the_fast_tier_for_real() {
    if !enabled() || std::env::var("OPENDOC_E2E_DOWNLOAD").as_deref() != Ok("1") {
        eprintln!("skipped: set OPENDOC_E2E_DOWNLOAD=1 as well");
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let data = std::env::var("OPENDOC_E2E_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| scratch.path().join("data"));
    let paths = AppPaths::new(data.clone(), data.join("logs"));
    paths.ensure().unwrap();
    let calibration =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/calibration-page.png");
    let core = Core::assemble(
        paths,
        engine_source(),
        uv(),
        calibration,
        Arc::new(NullSink),
    )
    .unwrap();
    tokio::spawn(core.queue.clone().inspect_loop());
    tokio::spawn(core.queue.clone().run_loop());
    let svc = &core.svc;

    // The CPU build of llama-server: download, verify, unpack, run.
    let started = Instant::now();
    let build = svc
        .manifest
        .runtimes
        .llama_server
        .builds
        .get("windows-x64-cpu")
        .expect("a CPU build in the manifest");
    let downloads = svc.paths.runtime.join("downloads");
    let control = Control::new();
    let mut archives = Vec::new();
    for file in &build.files {
        let item = DownloadFile {
            dest: downloads.join(&file.name),
            urls: vec![file.url.clone()],
            sha256: file.sha256.clone(),
            size: file.size_bytes,
        };
        svc.downloader
            .download(&item, &control, &|_| {})
            .await
            .unwrap_or_else(|e| panic!("download {}: {e:?}", file.name));
        archives.push(item.dest);
    }
    let dir = runtime::build_dir(&svc.paths.runtime, &svc.manifest, "windows-x64-cpu");
    let (manifest, target, unpack) = (svc.manifest.clone(), dir.clone(), archives.clone());
    tokio::task::spawn_blocking(move || runtime::install_archives(&unpack, &target, &manifest))
        .await
        .unwrap()
        .unwrap();
    let server = runtime::installed_server(&dir, &svc.manifest).expect("llama-server installed");
    let version = std::process::Command::new(&server)
        .arg("--version")
        .output()
        .expect("llama-server runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    assert!(text.contains("11669"), "llama-server --version: {text}");
    eprintln!(
        "[e2e] CPU runtime download, verify and unpack: {:.1} s",
        started.elapsed().as_secs_f64()
    );

    // First-run setup for the Fast tier: engine environment, model download, calibration.
    let started = Instant::now();
    let id = "fast-ppocrv6-tiny";
    core.setup.start(vec![id.to_string()]).await.unwrap();
    let setup = core.setup.clone();
    let deadline = Instant::now() + Duration::from_secs(3600);
    while setup.is_running() {
        assert!(Instant::now() < deadline, "setup timed out");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let status = core.setup.status().await;
    assert_eq!(status.phase, SetupPhase::Ready, "{:?}", status.error);
    assert!(status.calibration.is_some_and(|c| c.seconds_per_page > 0.0));
    assert!(svc.model_installed(id));
    assert!(svc.engine_env.is_ready());
    eprintln!(
        "[e2e] first-run setup (engine env, Fast tier download, calibration): {:.1} s",
        started.elapsed().as_secs_f64()
    );

    // And the freshly downloaded models convert a page.
    let page = PathBuf::from(r"G:\opendoc-spike\samples\irs-fw9\page01_200dpi.png");
    let work = scratch.path().join("docs");
    std::fs::create_dir_all(&work).unwrap();
    let copy = work.join("form.png");
    std::fs::copy(page, &copy).unwrap();
    let queue = core.queue.clone();
    let paths = vec![copy.to_string_lossy().into_owned()];
    let added = tokio::task::spawn_blocking(move || queue.add(paths))
        .await
        .unwrap()
        .unwrap();
    core.queue
        .start(ConvertOptions {
            formats: vec![ExportFormat::Docx, ExportFormat::Markdown],
            model_id: id.to_string(),
            save_to: SaveLocation::Original,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let item = core
            .queue
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.file_id == added[0].file_id)
            .unwrap();
        if item.status == QueueStatus::Done {
            break;
        }
        assert!(
            !matches!(item.status, QueueStatus::Failed | QueueStatus::Cancelled),
            "{:?}",
            item.error
        );
        assert!(Instant::now() < deadline, "conversion timed out");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(work.join("form.docx").is_file() && work.join("form.md").is_file());
    core.svc.supervisor.shutdown().await;
}
