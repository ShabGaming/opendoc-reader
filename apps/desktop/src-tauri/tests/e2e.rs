//! The whole app minus the webview: the real core (queue, setup, supervisor) with the real
//! engine environment, the real llama-server and the real models, driven through the same
//! functions the Tauri commands call.
//!
//! ```text
//! OPENDOC_INTEGRATION=1 cargo test --test e2e -- --nocapture
//! ```
//!
//! Settings (all optional):
//! - `OPENDOC_E2E_DATA`: keep the app-data folder there, so the engine environment, the models
//!   and the runtime are reused by the next run (and by `tauri dev` with `OPENDOC_APP_DATA`).
//!   Default: a fresh temporary folder.
//! - `OPENDOC_E2E_OUT`: where the input files are copied and the outputs are written.
//! - `OPENDOC_E2E_TIERS`: comma-separated subset of `fast,accurate,max`.
//! - `OPENDOC_E2E_MODELS`: models in the manifest layout (default `G:\opendoc-dev\models`, built
//!   by `engine/scripts/link_dev_models.py`); they are brought in with `models_import`.
//! - `OPENDOC_E2E_LLAMA`: a llama-server build folder to place as the runtime (default: the
//!   spike's `bin\llama-cuda` or `bin\llama-cpu`, whichever matches this machine's backend).
//! - `OPENDOC_E2E_SAMPLES`: the spike samples folder (default `G:\opendoc-spike\samples`).
//! - `OPENDOC_E2E_PYTHON`: a Python with the engine's dev dependencies (default
//!   `engine/.venv`), used to build the inputs and to check the outputs.

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{enabled, engine_source, uv};
use opendoc_reader_lib::app_core::Core;
use opendoc_reader_lib::contract::{
    AppErrorCode, ConvertOptions, ExportFormat, QueueItem, QueueStatus, SaveLocation, SetupPhase,
    TextMode, EVENT_QUEUE_PROGRESS, EVENT_SETUP_PROGRESS,
};
use opendoc_reader_lib::paths::AppPaths;
use opendoc_reader_lib::runtime;
use opendoc_reader_lib::services::EventSink;
use serde_json::Value;

struct Tier {
    name: &'static str,
    model: &'static str,
    engine: &'static str,
}

const TIERS: [Tier; 3] = [
    Tier {
        name: "fast",
        model: "fast-ppocrv6-tiny",
        engine: "paddleocr-structure",
    },
    Tier {
        name: "accurate",
        model: "accurate-paddleocr-vl-1.6",
        engine: "paddleocr-vl",
    },
    Tier {
        name: "max",
        model: "max-deepseek-ocr-1-q8",
        engine: "deepseek-ocr",
    },
];

#[derive(Default)]
struct CollectSink(Mutex<Vec<(String, Value)>>);

impl EventSink for CollectSink {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_string(), payload));
    }
}

impl CollectSink {
    fn count(&self, event: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == event)
            .count()
    }
}

struct Rig {
    core: Arc<Core>,
    sink: Arc<CollectSink>,
    python: PathBuf,
    out: PathBuf,
    report: String,
}

fn env_path(name: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| default.to_string()))
}

fn log(rig: &mut Rig, line: impl AsRef<str>) {
    eprintln!("[e2e] {}", line.as_ref());
    let _ = writeln!(rig.report, "{}", line.as_ref());
}

async fn wait_until<T>(what: &str, timeout: Duration, mut check: impl FnMut() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out after {:?} waiting for {what}",
            timeout
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

fn items(core: &Core) -> Vec<QueueItem> {
    core.queue.list().unwrap()
}

fn item(core: &Core, file_id: &str) -> QueueItem {
    items(core)
        .into_iter()
        .find(|i| i.file_id == file_id)
        .expect("item exists")
}

/// Runs the helper script and returns its JSON output; panics with the message on failure.
fn helper(python: &Path, args: &[&str]) -> Value {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("e2e_helper.py");
    let output = std::process::Command::new(python)
        .arg("-I")
        .arg(&script)
        .args(args)
        .output()
        .expect("the helper python starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "helper {args:?} failed: {stdout} {stderr}"
    );
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("helper output {stdout}: {e}"))
}

/// Hard-links a folder tree (copies where linking is not possible).
fn link_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            link_tree(&entry.path(), &target);
        } else if std::fs::hard_link(entry.path(), &target).is_err() {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

impl Rig {
    async fn new(data: PathBuf, out: PathBuf) -> Rig {
        let paths = AppPaths::new(data.clone(), data.join("logs"));
        paths.ensure().unwrap();
        // Keep the log writer for the life of the process; the file is the first place to look
        // when a run fails.
        std::mem::forget(opendoc_reader_lib::logging::init(&paths.logs));
        let sink = Arc::new(CollectSink::default());
        let calibration =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/calibration-page.png");
        let core = Core::assemble(paths, engine_source(), uv(), calibration, sink.clone()).unwrap();
        tokio::spawn(core.queue.clone().inspect_loop());
        tokio::spawn(core.queue.clone().run_loop());
        let python = std::env::var("OPENDOC_E2E_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let venv = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine/.venv");
                if cfg!(windows) {
                    venv.join("Scripts/python.exe")
                } else {
                    venv.join("bin/python")
                }
            });
        Rig {
            core,
            sink,
            python,
            out,
            report: String::new(),
        }
    }

    // -------------------------------------------------------------------------------------
    // Setup

    async fn place_runtime(&mut self) {
        let svc = &self.core.svc;
        let (key, dir) = svc
            .runtime_location()
            .await
            .expect("a build for this machine");
        let backend = svc.backend().await;
        if runtime::installed_server(&dir, &svc.manifest).is_some() {
            log(self, format!("runtime {key} already installed"));
            return;
        }
        let source = match std::env::var("OPENDOC_E2E_LLAMA") {
            Ok(path) => PathBuf::from(path),
            Err(_) if key.ends_with("cuda") => PathBuf::from(r"G:\opendoc-spike\bin\llama-cuda"),
            Err(_) if key.ends_with("cpu") => PathBuf::from(r"G:\opendoc-spike\bin\llama-cpu"),
            Err(_) => panic!("no local llama-server for {key}; set OPENDOC_E2E_LLAMA"),
        };
        link_tree(&source, &dir);
        runtime::mark_installed(&dir, &svc.manifest).unwrap();
        assert!(runtime::installed_server(&dir, &svc.manifest).is_some());
        log(
            self,
            format!(
                "runtime {key} ({backend:?}) placed from {}",
                source.display()
            ),
        );
    }

    async fn import_models(&mut self, models: &Path, ids: &[&str]) {
        for id in ids {
            let started = Instant::now();
            let already = self.core.svc.model_installed(id);
            self.core
                .setup
                .import((*id).to_string(), models.to_string_lossy().into_owned())
                .await
                .unwrap_or_else(|e| panic!("import {id}: {e:?}"));
            let core = self.core.clone();
            wait_until("the import to finish", Duration::from_secs(1800), || {
                (!core.setup.is_running()).then_some(())
            })
            .await;
            assert!(
                self.core.svc.model_installed(id),
                "{id} is installed after the import"
            );
            let listed = self.core.models_list();
            let info = listed.iter().find(|m| m.id == *id).unwrap();
            assert_eq!(info.installed_size_bytes, Some(info.download_size_bytes));
            log(
                self,
                format!(
                    "models_import {id}: {:.1} s{}",
                    started.elapsed().as_secs_f64(),
                    if already { " (was installed)" } else { "" }
                ),
            );
        }
    }

    /// First-run setup as the app runs it: engine environment with uv, the models already
    /// installed, then calibration on the first model.
    async fn finish_setup(&mut self, model: &str) {
        let started = Instant::now();
        self.core
            .setup
            .start(vec![model.to_string()])
            .await
            .unwrap();
        let core = self.core.clone();
        wait_until("setup", Duration::from_secs(3600), || {
            (!core.setup.is_running()).then_some(())
        })
        .await;
        let status = self.core.setup.status().await;
        assert!(
            status.phase == SetupPhase::Ready,
            "setup ended in {:?}: {:?}",
            status.phase,
            status.error
        );
        let calibration = status.calibration.expect("calibration ran");
        assert_eq!(calibration.model_id, model);
        assert!(calibration.seconds_per_page > 0.0);
        assert!(self.core.svc.engine_env.is_ready());
        log(
            self,
            format!(
                "setup (engine env, calibration): {:.1} s; calibration {:.2} s/page on {model}",
                started.elapsed().as_secs_f64(),
                calibration.seconds_per_page
            ),
        );
    }

    // -------------------------------------------------------------------------------------
    // Converting

    fn options(&self, tier: &Tier) -> ConvertOptions {
        ConvertOptions {
            formats: vec![ExportFormat::Docx, ExportFormat::Markdown],
            model_id: tier.model.to_string(),
            save_to: SaveLocation::Original,
        }
    }

    async fn add(&self, files: &[PathBuf]) -> Vec<QueueItem> {
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let core = self.core.clone();
        tokio::task::spawn_blocking(move || core.queue.add(paths))
            .await
            .unwrap()
            .unwrap()
    }

    /// Waits until every item of the batch is in a terminal state and returns them.
    async fn wait_finished(&self, batch: &[QueueItem], timeout: Duration) -> Vec<QueueItem> {
        let core = self.core.clone();
        let ids: Vec<String> = batch.iter().map(|i| i.file_id.clone()).collect();
        wait_until("the batch to finish", timeout, || {
            let now: Vec<QueueItem> = ids.iter().map(|id| item(&core, id)).collect();
            now.iter()
                .all(|i| {
                    matches!(
                        i.status,
                        QueueStatus::Done
                            | QueueStatus::Failed
                            | QueueStatus::Cancelled
                            | QueueStatus::NeedsAttention
                    )
                })
                .then_some(now)
        })
        .await
    }

    fn assert_done(&self, finished: &[QueueItem]) {
        for i in finished {
            assert_eq!(
                i.status,
                QueueStatus::Done,
                "{} ended {:?}: {:?}",
                i.name,
                i.status,
                i.error
            );
        }
    }

    /// The checks every finished conversion has to pass; `suffix` is "" for the first
    /// conversion of a name and " (2)" for the second.
    async fn verify(&mut self, tier: &Tier, i: &QueueItem, suffix: &str) {
        let paths = &self.core.svc.paths;
        let pages = i.page_count.expect("inspected");
        assert_eq!(i.pages_done, pages, "{}", i.name);
        let original = PathBuf::from(&i.path);
        let dir = original.parent().unwrap();
        let stem = original.file_stem().unwrap().to_string_lossy().into_owned();

        // Outputs next to the original, with the right names.
        let docx = dir.join(format!("{stem}{suffix}.docx"));
        let markdown = dir.join(format!("{stem}{suffix}.md"));
        assert!(docx.is_file(), "missing {}", docx.display());
        assert!(markdown.is_file(), "missing {}", markdown.display());
        let reported: Vec<String> = i.outputs.iter().map(|o| o.path.clone()).collect();
        assert!(
            reported.contains(&docx.to_string_lossy().into_owned())
                && reported.contains(&markdown.to_string_lossy().into_owned()),
            "outputs reported: {reported:?}"
        );
        let docx_info = helper(&self.python, &["check-docx", &docx.to_string_lossy()]);
        let text = std::fs::read_to_string(&markdown).unwrap();
        assert!(
            text.trim().len() > 20,
            "markdown is nearly empty for {}",
            i.name
        );

        // Page results: files on disk, valid against the schema, readable through get_review.
        let ir_files: Vec<String> = (0..pages)
            .map(|p| paths.page_ir(&i.file_id, p).to_string_lossy().into_owned())
            .collect();
        let mut args = vec!["check-ir"];
        args.extend(ir_files.iter().map(String::as_str));
        let ir_info = helper(&self.python, &args);
        let states = self.core.queue.pages(&i.file_id).unwrap();
        assert!(states
            .iter()
            .all(|p| p.state == opendoc_reader_lib::contract::PageState::Done
                && p.image_path.is_some()));
        for page in 0..pages {
            let review = self.core.queue.review(&i.file_id, page).await.unwrap();
            assert_eq!(review.page_index, page);
            assert_eq!(review.page_count, pages);
            assert!(Path::new(&review.image_path).is_file());
            assert_eq!(review.engine.id, tier.engine, "engine of {}", i.name);
            assert_eq!(review.engine.model, tier.model);
            assert!(review.page["blocks"].is_array());
        }
        let seconds = i.duration_ms.unwrap_or(0) as f64 / 1000.0;
        log(
            self,
            format!(
                "  {} [{}]: {pages} page(s), {seconds:.1} s ({:.1} s/page), docx {docx_info}, ir {ir_info}, md {} chars",
                i.name,
                tier.name,
                seconds / f64::from(pages.max(1)),
                text.len()
            ),
        );
    }

    async fn run_tier(&mut self, tier: &Tier, inputs: &[PathBuf]) {
        let dir = self.out.join(tier.name);
        std::fs::create_dir_all(&dir).unwrap();
        let files: Vec<PathBuf> = inputs
            .iter()
            .map(|f| {
                let target = dir.join(f.file_name().unwrap());
                std::fs::copy(f, &target).unwrap();
                target
            })
            .collect();
        log(self, format!("tier {} ({})", tier.name, tier.model));

        // First conversion of both files.
        let started = Instant::now();
        let batch = self.add(&files).await;
        assert_eq!(batch.len(), files.len());
        self.core.queue.start(self.options(tier)).unwrap();
        let finished = self.wait_finished(&batch, Duration::from_secs(3600)).await;
        self.assert_done(&finished);
        log(
            self,
            format!(
                "  first run wall time {:.1} s",
                started.elapsed().as_secs_f64()
            ),
        );
        for i in &finished {
            self.verify(tier, i, "").await;
        }

        // Second conversion: names are taken, so the outputs get " (2)". Fast does both files,
        // the slower tiers only the image.
        let again: Vec<PathBuf> = if tier.name == "fast" {
            files.clone()
        } else {
            files
                .iter()
                .filter(|f| f.extension().is_some_and(|e| e == "png"))
                .cloned()
                .collect()
        };
        self.core.queue.clear_done().unwrap();
        let started = Instant::now();
        let batch = self.add(&again).await;
        self.core.queue.start(self.options(tier)).unwrap();
        let finished = self.wait_finished(&batch, Duration::from_secs(3600)).await;
        self.assert_done(&finished);
        log(
            self,
            format!(
                "  second run wall time {:.1} s",
                started.elapsed().as_secs_f64()
            ),
        );
        for i in &finished {
            self.verify(tier, i, " (2)").await;
        }

        // `export` writes one more format into the next free name.
        if tier.name == "fast" {
            let last = &finished[0];
            let output = self
                .core
                .queue
                .export(&last.file_id, ExportFormat::Docx)
                .await
                .unwrap();
            let stem = Path::new(&last.path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert!(
                output.path.ends_with(&format!("{stem} (3).docx")),
                "export wrote {}",
                output.path
            );
            assert!(Path::new(&output.path).is_file());
            log(self, "  export docx -> \"(3)\" name ok");
        }
        self.core.queue.clear_done().unwrap();
    }

    /// Cancel in the middle of a multi-page file, then retry: the finished pages are kept.
    async fn cancel_and_retry(&mut self, tier: &Tier, pdf: &Path) {
        let dir = self.out.join(format!("cancel-{}", tier.name));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join(pdf.file_name().unwrap());
        std::fs::copy(pdf, &target).unwrap();
        log(
            self,
            format!("cancel and retry on {} [{}]", pdf.display(), tier.name),
        );

        let batch = self.add(std::slice::from_ref(&target)).await;
        let id = batch[0].file_id.clone();
        self.core.queue.start(self.options(tier)).unwrap();
        let core = self.core.clone();
        let first_done = wait_until("the first page", Duration::from_secs(900), || {
            let now = item(&core, &id);
            assert!(
                !matches!(now.status, QueueStatus::Failed),
                "failed early: {:?}",
                now.error
            );
            (now.pages_done >= 1 && now.status == QueueStatus::Converting).then_some(now)
        })
        .await;
        let total = first_done.page_count.unwrap();
        assert!(total >= 3);
        let first_ir = self.core.svc.paths.page_ir(&id, 0);
        let first_modified = std::fs::metadata(&first_ir).unwrap().modified().unwrap();

        let started = Instant::now();
        self.core.queue.cancel(Some(id.clone())).await.unwrap();
        let core = self.core.clone();
        let cancelled = wait_until("the cancel to land", Duration::from_secs(120), || {
            let now = item(&core, &id);
            (now.status != QueueStatus::Converting).then_some(now)
        })
        .await;
        assert_eq!(
            cancelled.status,
            QueueStatus::Cancelled,
            "{:?}",
            cancelled.error
        );
        assert!(cancelled.outputs.is_empty());
        assert!(
            cancelled.pages_done < total,
            "the file finished before the cancel landed"
        );
        log(
            self,
            format!(
                "  cancelled after {} of {total} pages, took {:.1} s to stop",
                cancelled.pages_done,
                started.elapsed().as_secs_f64()
            ),
        );
        // The engine still answers after a cancel.
        assert!(self
            .core
            .queue
            .pages(&id)
            .unwrap()
            .iter()
            .any(|p| p.state == opendoc_reader_lib::contract::PageState::Done));

        self.core.queue.retry(&id).unwrap();
        let finished = self.wait_finished(&batch, Duration::from_secs(900)).await;
        self.assert_done(&finished);
        assert_eq!(
            std::fs::metadata(&first_ir).unwrap().modified().unwrap(),
            first_modified,
            "page 0 was converted again instead of resumed"
        );
        self.verify(tier, &finished[0], "").await;
        log(self, "  retry resumed and finished");
        self.core.queue.clear_done().unwrap();
    }

    /// A PDF with a text layer waits for a decision; both answers work.
    async fn born_digital(&mut self, tier: &Tier, pdf: &Path) {
        let dir = self.out.join("born-digital");
        std::fs::create_dir_all(&dir).unwrap();
        log(self, "born-digital PDF");
        self.core.queue.start(self.options(tier)).unwrap();

        for (mode, folder) in [
            (TextMode::ExistingText, "existing-text"),
            (TextMode::Ocr, "ocr"),
        ] {
            let sub = dir.join(folder);
            std::fs::create_dir_all(&sub).unwrap();
            let target = sub.join(pdf.file_name().unwrap());
            std::fs::copy(pdf, &target).unwrap();
            let batch = self.add(std::slice::from_ref(&target)).await;
            let id = batch[0].file_id.clone();
            let core = self.core.clone();
            let waiting = wait_until("needsAttention", Duration::from_secs(300), || {
                let now = item(&core, &id);
                (now.status != QueueStatus::Waiting).then_some(now)
            })
            .await;
            assert_eq!(
                waiting.status,
                QueueStatus::NeedsAttention,
                "{:?}",
                waiting.error
            );
            assert_eq!(waiting.has_text_layer, Some(true));
            assert_eq!(waiting.page_count, Some(2));

            let started = Instant::now();
            self.core.queue.set_text_mode(&id, mode).unwrap();
            let finished = self.wait_finished(&batch, Duration::from_secs(600)).await;
            self.assert_done(&finished);
            let markdown = std::fs::read_to_string(sub.join("born-digital.md")).unwrap();
            assert!(
                markdown.contains("Quarterly report"),
                "{folder}: {markdown}"
            );
            assert!(markdown.contains("Revenue grew"), "{folder}: {markdown}");
            let review = self.core.queue.review(&id, 0).await.unwrap();
            if mode == TextMode::ExistingText {
                assert_eq!(review.engine.id, "pdf-text");
            } else {
                assert_eq!(review.engine.id, tier.engine);
            }
            log(
                self,
                format!(
                    "  {folder}: done in {:.1} s, markdown has the text",
                    started.elapsed().as_secs_f64()
                ),
            );
            self.verify_outputs_only(&finished[0]);
        }
        self.core.queue.clear_done().unwrap();
    }

    fn verify_outputs_only(&self, i: &QueueItem) {
        let original = PathBuf::from(&i.path);
        let stem = original.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = original.parent().unwrap();
        helper(
            &self.python,
            &[
                "check-docx",
                &dir.join(format!("{stem}.docx")).to_string_lossy(),
            ],
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_whole_app_converts_documents_with_every_tier() {
    if !enabled() {
        return;
    }
    let total = Instant::now();
    let keep = std::env::var("OPENDOC_E2E_DATA").ok().map(PathBuf::from);
    let scratch = tempfile::tempdir().unwrap();
    let data = keep.clone().unwrap_or_else(|| scratch.path().join("data"));
    let out = std::env::var("OPENDOC_E2E_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| scratch.path().join("out"));
    std::fs::create_dir_all(&out).unwrap();
    let tiers: Vec<&Tier> = match std::env::var("OPENDOC_E2E_TIERS") {
        Ok(list) => TIERS
            .iter()
            .filter(|t| list.split(',').any(|n| n.trim() == t.name))
            .collect(),
        Err(_) => TIERS.iter().collect(),
    };

    let mut rig = Rig::new(data, out.clone()).await;
    let models = env_path("OPENDOC_E2E_MODELS", r"G:\opendoc-dev\models");
    let samples = env_path("OPENDOC_E2E_SAMPLES", r"G:\opendoc-spike\samples");

    // Setup. Nothing is installed in a fresh folder.
    let before = rig.core.setup.status().await;
    log(&mut rig, format!("setup status before: {:?}", before.phase));
    rig.place_runtime().await;
    let ids: Vec<&str> = tiers.iter().map(|t| t.model).collect();
    rig.import_models(&models, &ids).await;
    let calibrated = tiers.first().expect("at least one tier").model;
    rig.finish_setup(calibrated).await;
    assert!(rig.sink.count(EVENT_SETUP_PROGRESS) > 0);

    // Inputs: a three-page scanned PDF, a PNG, and a born-digital PDF.
    let inputs_dir = out.join("inputs");
    let made = helper(
        &rig.python,
        &[
            "make-inputs",
            &inputs_dir.to_string_lossy(),
            &samples.to_string_lossy(),
        ],
    );
    let files: Vec<PathBuf> = made["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| PathBuf::from(f.as_str().unwrap()))
        .collect();
    let (scanned, png, born) = (&files[0], &files[1], &files[2]);

    // A file the engine cannot read fails on its own and does not stop the others.
    let broken = out.join("broken.pdf");
    std::fs::write(&broken, b"this is not a pdf").unwrap();
    let batch = rig.add(std::slice::from_ref(&broken)).await;
    let finished = rig.wait_finished(&batch, Duration::from_secs(300)).await;
    assert_eq!(finished[0].status, QueueStatus::Failed);
    let error = finished[0].error.clone().unwrap();
    assert_eq!(error.code, AppErrorCode::BadInput, "{error:?}");
    log(
        &mut rig,
        format!("broken.pdf fails as badInput ({:?})", error.reason),
    );
    rig.core.queue.remove(&finished[0].file_id).unwrap();

    for tier in &tiers {
        rig.run_tier(tier, &[scanned.clone(), png.clone()]).await;
    }
    for tier in &tiers {
        rig.cancel_and_retry(tier, scanned).await;
    }
    rig.born_digital(tiers[0], born).await;

    assert!(rig.sink.count(EVENT_QUEUE_PROGRESS) > 0);
    let events = rig.sink.count(EVENT_QUEUE_PROGRESS);
    log(&mut rig, format!("queue events: {events}"));
    log(
        &mut rig,
        format!("total {:.0} s", total.elapsed().as_secs_f64()),
    );

    // Nothing is left running.
    rig.core.svc.supervisor.shutdown().await;
    assert!(rig.core.svc.supervisor.running_model().await.is_none());
    let _ = std::fs::write(out.join("e2e-report.txt"), &rig.report);
    if keep.is_none() {
        drop(scratch);
    }
}
