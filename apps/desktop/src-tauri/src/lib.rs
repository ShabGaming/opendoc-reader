//! OpenDoc Reader desktop core.
//!
//! The Rust side owns the job queue, process supervision, downloads, the
//! hardware probe and settings.

pub mod commands;
pub mod contract;
pub mod error;
pub mod hardware;
pub mod logging;
pub mod manifest;
pub mod paths;
pub mod settings;
pub mod suggester;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_version])
        .run(tauri::generate_context!())
        .expect("error while running OpenDoc Reader");
}
