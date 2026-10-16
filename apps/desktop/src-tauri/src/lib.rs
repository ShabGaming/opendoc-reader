//! OpenDoc Reader desktop core.
//!
//! The Rust side owns the job queue, process supervision, downloads, the
//! hardware probe and settings.

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
pub mod runtime;
pub mod services;
pub mod settings;
pub mod setup;
pub mod suggester;
pub mod supervisor;
#[cfg(test)]
pub mod testutil;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_version])
        .run(tauri::generate_context!())
        .expect("error while running OpenDoc Reader");
}
