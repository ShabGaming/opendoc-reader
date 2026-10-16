//! Commands callable from the frontend through `invoke` (docs/protocol.md, Part 2).
//!
//! Every command is a thin wrapper over [`crate::core::Core`], which the app registers as
//! managed state. Names and argument names follow the contract: Rust `snake_case`, JavaScript
//! `camelCase` (`invoke("queue_cancel", { fileId })`).

mod queue;
mod setup;
mod system;

pub use queue::*;
pub use setup::*;
pub use system::*;

use std::sync::Arc;

use tauri::State;

use crate::app_core::Core;

pub type CoreState<'a> = State<'a, Arc<Core>>;

/// The app version, taken from the crate metadata so it cannot drift.
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_matches_the_crate_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn app_version_is_a_three_part_number() {
        let version = app_version();
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
