//! Shared by the opt-in integration tests.

use std::path::PathBuf;

/// True when `OPENDOC_INTEGRATION=1`; says so on stderr otherwise.
pub fn enabled() -> bool {
    if std::env::var("OPENDOC_INTEGRATION").as_deref() == Ok("1") {
        true
    } else {
        eprintln!("skipped: set OPENDOC_INTEGRATION=1 to run");
        false
    }
}

/// A uv to run: `OPENDOC_UV`, then the sidecar in `binaries/` (skipping the empty placeholder
/// the build script writes when `scripts/fetch-uv.mjs` has not run), then `uv` on the PATH.
pub fn uv() -> PathBuf {
    if let Ok(path) = std::env::var("OPENDOC_UV") {
        return PathBuf::from(path);
    }
    let sidecars = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    if let Ok(entries) = std::fs::read_dir(&sidecars) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("uv-")
                && !name.ends_with(".stamp")
                && entry.metadata().is_ok_and(|m| m.is_file() && m.len() > 0)
            {
                return entry.path();
            }
        }
    }
    PathBuf::from("uv")
}

/// The engine project in this checkout.
pub fn engine_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine")
}
