//! OpenDoc Reader desktop core.
//!
//! The Rust side owns the job queue, process supervision, downloads, the hardware probe and
//! settings. The UI reaches it through the commands in [`commands`]; the contract is
//! `docs/protocol.md`, Part 2.

pub mod app_core;
pub mod commands;
pub mod contract;
pub mod download;
pub mod engine_env;
pub mod error;
#[cfg(test)]
pub mod fakes;
pub mod hardware;
pub mod logging;
pub mod manifest;
pub mod models;
pub mod paths;
pub mod queue;
pub mod runtime;
pub mod services;
pub mod settings;
pub mod setup;
pub mod suggester;
pub mod supervisor;
#[cfg(test)]
pub mod testutil;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::{Emitter, Manager, RunEvent};

use crate::app_core::Core;
use crate::contract::{AppError, AppErrorCode};
use crate::download::Downloader;
use crate::engine_env::EngineEnv;
use crate::manifest::Manifest;
use crate::paths::AppPaths;
use crate::services::{CalibrationStore, EventSink, HardwareService, Services};
use crate::settings::SettingsStore;
use crate::supervisor::{Dirs, EngineSpawn, SpawnFactory, Supervisor, SupervisorConfig};

/// Sends events to the main window.
struct TauriSink(tauri::AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: &str, payload: Value) {
        if let Err(error) = self.0.emit_to("main", event, payload) {
            tracing::warn!("could not emit {event}: {error}");
        }
    }
}

/// Keeps the log writer alive for the life of the app.
struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

fn calibration_page(resource_dir: Option<&std::path::Path>) -> PathBuf {
    if let Some(dir) = resource_dir {
        let bundled = dir.join("calibration-page.png");
        if bundled.is_file() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/calibration-page.png")
}

fn build_core(app: &tauri::App) -> Result<Arc<Core>, Box<dyn std::error::Error>> {
    let data = app.path().app_data_dir()?;
    let logs = app.path().app_log_dir()?;
    let paths = AppPaths::new(data, logs);
    paths.ensure()?;
    app.manage(LogGuard(logging::init(&paths.logs)));
    tracing::info!("OpenDoc Reader {} starting", env!("CARGO_PKG_VERSION"));

    // The review screen shows page images from the work folder through the asset protocol.
    app.asset_protocol_scope()
        .allow_directory(&paths.work, true)?;

    let resource_dir = app.path().resource_dir().ok();
    let source = engine_env::resolve_source(resource_dir.as_deref())
        .unwrap_or_else(|| paths.engine.join("missing-source"));
    let engine_env = EngineEnv::new(&paths, source, engine_env::resolve_uv());

    let settings = SettingsStore::load(paths.settings_file.clone(), &paths.default_models);
    let models_dir = PathBuf::from(settings.get().models_dir);

    let spawn_env = engine_env.clone();
    let spawn: SpawnFactory = Arc::new(move || {
        if !spawn_env.is_ready() {
            return Err(AppError::new(
                AppErrorCode::EngineUnavailable,
                "The document engine is not installed yet.",
            )
            .with_reason("engine_not_installed"));
        }
        Ok(EngineSpawn {
            program: spawn_env.python(),
            args: vec!["-m".into(), "opendoc_engine".into()],
            cwd: Some(spawn_env.project_dir()),
            env: Vec::new(),
        })
    });
    let supervisor = Supervisor::new(
        spawn,
        Dirs {
            work_dir: paths.work.clone(),
            models_dir,
        },
        SupervisorConfig::default(),
    );

    let svc = Services {
        calibration: CalibrationStore::load(paths.calibration_file.clone()),
        settings,
        manifest: Arc::new(Manifest::embedded().clone()),
        hardware: HardwareService::new(),
        downloader: Downloader::new()?,
        engine_env: engine_env.clone(),
        supervisor,
        calibration_page: calibration_page(resource_dir.as_deref()),
        sink: Arc::new(TauriSink(app.handle().clone())),
        paths,
    };
    let core = Arc::new(Core::new(Arc::new(svc))?);

    // Keep the engine source in step with this version of the app.
    if engine_env.is_ready() {
        let env = engine_env.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = env.refresh_source() {
                tracing::warn!("could not refresh the engine source: {error}");
            }
        });
    }
    tauri::async_runtime::spawn(core.queue.clone().inspect_loop());
    tauri::async_runtime::spawn(core.queue.clone().run_loop());
    core.queue.inspect_pending();
    Ok(core)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let core = build_core(app)?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::probe_hardware,
            commands::get_recommendation,
            commands::start_setup_download,
            commands::setup_pause,
            commands::setup_resume,
            commands::setup_status,
            commands::models_list,
            commands::models_install,
            commands::models_remove,
            commands::models_import,
            commands::queue_add,
            commands::queue_list,
            commands::queue_start,
            commands::queue_pause,
            commands::queue_resume,
            commands::queue_cancel,
            commands::queue_remove,
            commands::queue_clear_done,
            commands::queue_retry,
            commands::queue_set_text_mode,
            commands::get_pages,
            commands::get_review,
            commands::export,
            commands::reveal_in_folder,
            commands::settings_get,
            commands::settings_set,
            commands::get_diagnostics,
            commands::open_logs_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OpenDoc Reader");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Close the children cleanly; the job objects and process groups cover the case
            // where this does not get to run.
            if let Some(core) = handle.try_state::<Arc<Core>>() {
                let core = core.inner().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(8),
                        core.svc.supervisor.shutdown(),
                    )
                    .await;
                });
            }
        }
    });
}
