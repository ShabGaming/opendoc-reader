//! Settings, diagnostics and the OS file manager.

use std::path::Path;

use super::CoreState;
use crate::app_core::existing_path;
use crate::contract::{AppError, AppErrorCode, Settings, SettingsPatch};

#[tauri::command]
pub async fn settings_get(core: CoreState<'_>) -> Result<Settings, AppError> {
    Ok(core.svc.settings.get())
}

#[tauri::command]
pub async fn settings_set(core: CoreState<'_>, patch: SettingsPatch) -> Result<Settings, AppError> {
    core.settings_set(patch)
}

#[tauri::command]
pub async fn get_diagnostics(core: CoreState<'_>) -> Result<String, AppError> {
    core.diagnostics().await
}

#[tauri::command]
pub async fn open_logs_folder(core: CoreState<'_>) -> Result<(), AppError> {
    let dir = core.svc.paths.logs.clone();
    std::fs::create_dir_all(&dir)?;
    open_in_file_manager(&dir, false)
}

#[tauri::command]
pub async fn reveal_in_folder(path: String) -> Result<(), AppError> {
    let path = existing_path(&path)?;
    open_in_file_manager(&path, true)
}

/// Shows a folder, or selects a file in its folder when `select` is set.
fn open_in_file_manager(path: &Path, select: bool) -> Result<(), AppError> {
    let result =
        file_manager_command(path, select).and_then(|mut command| command.spawn().map(|_| ()));
    result.map_err(|e| {
        AppError::new(AppErrorCode::Io, "The file manager could not be opened.")
            .with_reason("open_failed")
            .with_detail(e.to_string())
    })
}

#[cfg(windows)]
fn file_manager_command(path: &Path, select: bool) -> std::io::Result<std::process::Command> {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new("explorer.exe");
    if select {
        // Explorer wants one argument, `/select,"<path>"`, with the quotes inside it; the usual
        // argument quoting of the standard library would break it.
        command.raw_arg(format!("/select,\"{}\"", path.display()));
    } else {
        command.arg(path);
    }
    Ok(command)
}

#[cfg(target_os = "macos")]
fn file_manager_command(path: &Path, select: bool) -> std::io::Result<std::process::Command> {
    let mut command = std::process::Command::new("open");
    if select {
        command.arg("-R");
    }
    command.arg(path);
    Ok(command)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn file_manager_command(path: &Path, _select: bool) -> std::io::Result<std::process::Command> {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(path);
    Ok(command)
}
