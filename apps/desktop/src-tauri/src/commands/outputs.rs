//! Saved outputs: copies to a chosen path, and opening what the queue wrote.

use std::path::PathBuf;

use super::CoreState;
use crate::contract::{AppError, AppErrorCode, ExportFormat, OutputFile, OutputStatus};

#[tauri::command]
pub async fn export_to(
    core: CoreState<'_>,
    file_id: String,
    format: ExportFormat,
    dest_path: String,
    overwrite: bool,
) -> Result<OutputFile, AppError> {
    core.queue
        .export_to(&file_id, format, &dest_path, overwrite)
        .await
}

#[tauri::command]
pub async fn check_outputs(
    core: CoreState<'_>,
    file_id: String,
) -> Result<Vec<OutputStatus>, AppError> {
    core.queue.check_outputs(&file_id)
}

/// Opens an output with the OS default app. Only paths the queue wrote are accepted, so the UI
/// cannot be turned into a way to launch arbitrary files.
#[tauri::command]
pub async fn open_output(core: CoreState<'_>, path: String) -> Result<(), AppError> {
    let path: PathBuf = core.queue.known_output(&path)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| {
        AppError::new(AppErrorCode::Io, "The file could not be opened.")
            .with_reason("open_failed")
            .with_detail(e.to_string())
    })
}
