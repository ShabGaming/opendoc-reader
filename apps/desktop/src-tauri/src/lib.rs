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
pub mod migrate;
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
use crate::paths::AppPaths;
use crate::services::EventSink;

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
    // Everything lives in <local data>/OpenDoc Reader. OPENDOC_APP_DATA points a development
    // build at another data folder (with its own logs). Installs from before this layout are
    // moved over; the notes are written to the log once it is running.
    let mut notes: Vec<String> = Vec::new();
    let (data, logs) = migrate::locate(
        &app.path().local_data_dir()?,
        &app.path().app_data_dir()?,
        &app.path().app_log_dir()?,
        std::env::var_os("OPENDOC_APP_DATA").as_deref(),
        &mut |note| notes.push(note.to_string()),
    );
    let paths = AppPaths::new(data, logs);
    paths.ensure()?;
    app.manage(LogGuard(logging::init(&paths.logs)));
    tracing::info!("OpenDoc Reader {} starting", env!("CARGO_PKG_VERSION"));
    for note in notes {
        tracing::info!("data folder: {note}");
    }

    // The review screen shows page images and crops from the work folder through the asset
    // protocol. tauri.conf.json covers the default folder; this covers the override and a
    // fallback to the old location.
    app.asset_protocol_scope()
        .allow_directory(&paths.data, true)?;

    let resource_dir = app.path().resource_dir().ok();
    let source = engine_env::resolve_source(resource_dir.as_deref())
        .unwrap_or_else(|| paths.engine.join("missing-source"));
    let core = Core::assemble(
        paths,
        source,
        engine_env::resolve_uv(),
        calibration_page(resource_dir.as_deref()),
        Arc::new(TauriSink(app.handle().clone())),
    )?;
    let engine_env = core.svc.engine_env.clone();

    // A models folder outside app data is not covered by the scope above.
    let models_dir = core.svc.models_dir();
    if !models_dir.starts_with(&core.svc.paths.data) {
        app.asset_protocol_scope()
            .allow_directory(&models_dir, true)?;
    }

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
        .plugin(tauri_plugin_opener::init())
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
            commands::export_to,
            commands::check_outputs,
            commands::open_output,
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
