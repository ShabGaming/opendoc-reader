//! The queue.

use super::CoreState;
use crate::contract::{
    AppError, ConvertOptions, ExportFormat, OutputFile, PageStatus, QueueItem, ReviewPage, TextMode,
};

#[tauri::command]
pub async fn queue_add(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<Vec<QueueItem>, AppError> {
    let queue = core.queue.clone();
    // Walking a dropped folder can take a moment; keep it off the async threads.
    tokio::task::spawn_blocking(move || queue.add(paths))
        .await
        .map_err(|e| AppError::internal("Adding files crashed.").with_detail(e.to_string()))?
}

#[tauri::command]
pub async fn queue_list(core: CoreState<'_>) -> Result<Vec<QueueItem>, AppError> {
    core.queue.list()
}

#[tauri::command]
pub async fn queue_start(core: CoreState<'_>, options: ConvertOptions) -> Result<(), AppError> {
    core.queue.start(options)
}

#[tauri::command]
pub async fn queue_pause(core: CoreState<'_>) -> Result<(), AppError> {
    core.queue.pause();
    Ok(())
}

#[tauri::command]
pub async fn queue_resume(core: CoreState<'_>) -> Result<(), AppError> {
    core.queue.resume();
    Ok(())
}

#[tauri::command]
pub async fn queue_cancel(core: CoreState<'_>, file_id: Option<String>) -> Result<(), AppError> {
    core.queue.cancel(file_id).await
}

#[tauri::command]
pub async fn queue_remove(core: CoreState<'_>, file_id: String) -> Result<(), AppError> {
    core.queue.remove(&file_id)
}

#[tauri::command]
pub async fn queue_clear_done(core: CoreState<'_>) -> Result<Vec<String>, AppError> {
    core.queue.clear_done()
}

#[tauri::command]
pub async fn queue_retry(core: CoreState<'_>, file_id: String) -> Result<(), AppError> {
    core.queue.retry(&file_id)
}

#[tauri::command]
pub async fn queue_set_text_mode(
    core: CoreState<'_>,
    file_id: String,
    mode: TextMode,
) -> Result<(), AppError> {
    core.queue.set_text_mode(&file_id, mode)
}

#[tauri::command]
pub async fn get_pages(core: CoreState<'_>, file_id: String) -> Result<Vec<PageStatus>, AppError> {
    core.queue.pages(&file_id)
}

#[tauri::command]
pub async fn get_review(
    core: CoreState<'_>,
    file_id: String,
    page: u32,
) -> Result<ReviewPage, AppError> {
    core.queue.review(&file_id, page).await
}

#[tauri::command]
pub async fn export(
    core: CoreState<'_>,
    file_id: String,
    format: ExportFormat,
) -> Result<OutputFile, AppError> {
    core.queue.export(&file_id, format).await
}
