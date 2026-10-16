use std::{env, fs, path::PathBuf};

/// Tauri refuses to compile when a declared external binary is missing, even for `cargo test` and
/// `cargo clippy`, which never run it. `scripts/fetch-uv.mjs` puts the real `uv` there before a
/// build or `tauri dev`; this only keeps plain cargo commands working on a fresh checkout.
fn ensure_sidecar_placeholder() {
    let Ok(target) = env::var("TARGET") else {
        return;
    };
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let sidecar = manifest_dir
        .join("binaries")
        .join(format!("uv-{target}{extension}"));
    if sidecar.exists() {
        return;
    }
    if let Some(dir) = sidecar.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::write(&sidecar, b"").is_ok() {
        println!(
            "cargo:warning=uv sidecar missing, wrote an empty placeholder; run `node scripts/fetch-uv.mjs` before building an app"
        );
    }
}

fn main() {
    ensure_sidecar_placeholder();
    println!("cargo:rerun-if-changed=../../../models/manifest.json");
    tauri_build::build()
}
