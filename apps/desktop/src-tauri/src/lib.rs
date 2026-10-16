//! OpenDoc Reader desktop core.
//!
//! The Rust side owns the job queue, process supervision, downloads, the
//! hardware probe and settings. Each of those arrives as its own module;
//! only the command layer exists so far.

pub mod commands;
pub mod contract;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_version])
        .run(tauri::generate_context!())
        .expect("error while running OpenDoc Reader");
}
