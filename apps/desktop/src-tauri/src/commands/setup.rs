//! Setup, hardware and models.

use super::CoreState;
use crate::contract::{AppError, HardwareInfo, ModelInfo, Recommendation, SetupStatus};

#[tauri::command]
pub async fn probe_hardware(core: CoreState<'_>) -> Result<HardwareInfo, AppError> {
    Ok(core.probe_hardware().await)
}

#[tauri::command]
pub async fn get_recommendation(core: CoreState<'_>) -> Result<Recommendation, AppError> {
    Ok(core.recommendation().await)
}

#[tauri::command]
pub async fn start_setup_download(
    core: CoreState<'_>,
    model_ids: Vec<String>,
) -> Result<(), AppError> {
    core.setup.start(model_ids).await
}

#[tauri::command]
pub async fn setup_pause(core: CoreState<'_>) -> Result<(), AppError> {
    core.setup.pause();
    Ok(())
}

#[tauri::command]
pub async fn setup_resume(core: CoreState<'_>) -> Result<(), AppError> {
    core.setup.resume();
    Ok(())
}

#[tauri::command]
pub async fn setup_status(core: CoreState<'_>) -> Result<SetupStatus, AppError> {
    Ok(core.setup.status().await)
}

#[tauri::command]
pub async fn models_list(core: CoreState<'_>) -> Result<Vec<ModelInfo>, AppError> {
    Ok(core.models_list())
}

#[tauri::command]
pub async fn models_install(core: CoreState<'_>, model_id: String) -> Result<(), AppError> {
    core.setup.install(model_id).await
}

#[tauri::command]
pub async fn models_remove(core: CoreState<'_>, model_id: String) -> Result<(), AppError> {
    core.models_remove(&model_id).await
}

#[tauri::command]
pub async fn models_import(
    core: CoreState<'_>,
    model_id: String,
    folder: String,
) -> Result<(), AppError> {
    core.setup.import(model_id, folder).await
}
