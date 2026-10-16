//! Models on disk: install state, removal and "import from folder" (M1-4).
//!
//! A model lives at `<modelsDir>/<modelId>/[<dir>/]<name>`. A file is only ever at that path
//! after its checksum was verified (downloads and imports write `<name>.part` first), so
//! "installed" means every file exists with the manifest's size.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::contract::{AppError, AppErrorCode, ModelState};
use crate::manifest::{ModelEntry, ModelFile};

pub fn model_dir(models_dir: &Path, id: &str) -> PathBuf {
    models_dir.join(id)
}

pub fn file_path(models_dir: &Path, id: &str, file: &ModelFile) -> PathBuf {
    model_dir(models_dir, id).join(file.relative_path())
}

fn has_size(path: &Path, size: u64) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == size)
}

pub fn is_installed(models_dir: &Path, id: &str, model: &ModelEntry) -> bool {
    model
        .files
        .iter()
        .all(|file| has_size(&file_path(models_dir, id, file), file.size_bytes))
}

pub fn installed_size(models_dir: &Path, id: &str, model: &ModelEntry) -> Option<u64> {
    is_installed(models_dir, id, model).then(|| model.download_size_bytes())
}

/// True when some file of the model has a `.part` file next to it (a download in progress or
/// interrupted).
pub fn has_partial(models_dir: &Path, id: &str, model: &ModelEntry) -> bool {
    model.files.iter().any(|file| {
        let path = file_path(models_dir, id, file);
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".part");
        path.with_file_name(name).exists()
    })
}

pub fn state(models_dir: &Path, id: &str, model: &ModelEntry, downloading: bool) -> ModelState {
    if is_installed(models_dir, id, model) {
        ModelState::Installed
    } else if downloading {
        ModelState::Downloading
    } else {
        ModelState::NotInstalled
    }
}

pub fn remove(models_dir: &Path, id: &str) -> Result<(), AppError> {
    let dir = model_dir(models_dir, id);
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Where an import looks for one file inside the chosen folder: the manifest layout
/// (`<dir>/<name>`), the same layout under a folder named like the model, or the bare file name.
fn import_candidates(folder: &Path, id: &str, file: &ModelFile) -> Vec<PathBuf> {
    let relative = file.relative_path();
    vec![
        folder.join(&relative),
        folder.join(id).join(&relative),
        folder.join(&file.name),
    ]
}

/// Progress of an import: which file (0-based) and how many of its bytes are done.
pub type ImportProgress<'a> = &'a (dyn Fn(usize, u64) + Send + Sync);

/// Copies every file of the model from `folder` into the models folder, checking each against
/// the manifest's SHA-256 while copying. Blocking.
///
/// A missing file is `badInput` (`file_not_found`); a file with the wrong content is
/// `checksumMismatch` and is not kept.
pub fn import_from_folder(
    models_dir: &Path,
    id: &str,
    model: &ModelEntry,
    folder: &Path,
    progress: ImportProgress<'_>,
) -> Result<(), AppError> {
    if !folder.is_dir() {
        return Err(
            AppError::new(AppErrorCode::BadInput, "The folder does not exist.")
                .with_reason("file_not_found"),
        );
    }
    // Look everything up first, so a missing file is reported before any copying starts.
    let mut sources = Vec::new();
    for file in &model.files {
        let source = import_candidates(folder, id, file)
            .into_iter()
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::BadInput,
                    "A model file is missing from the folder.",
                )
                .with_reason("file_not_found")
                .with_detail(file.name.clone())
            })?;
        sources.push(source);
    }

    for (index, (file, source)) in model.files.iter().zip(&sources).enumerate() {
        let dest = file_path(models_dir, id, file);
        if has_size(&dest, file.size_bytes) {
            progress(index, file.size_bytes);
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut part_name = dest.file_name().unwrap_or_default().to_os_string();
        part_name.push(".part");
        let part = dest.with_file_name(part_name);

        let copied = copy_and_hash(source, &part, |done| progress(index, done));
        let digest = match copied {
            Ok(digest) => digest,
            Err(error) => {
                let _ = fs::remove_file(&part);
                return Err(error.into());
            }
        };
        if !digest.eq_ignore_ascii_case(&file.sha256) {
            let _ = fs::remove_file(&part);
            return Err(AppError::new(
                AppErrorCode::ChecksumMismatch,
                "A file in the folder does not match the expected checksum.",
            )
            .with_reason("checksum_mismatch")
            .with_detail(file.name.clone()));
        }
        fs::rename(&part, &dest)?;
    }
    Ok(())
}

fn copy_and_hash(
    source: &Path,
    dest: &Path,
    mut on_bytes: impl FnMut(u64),
) -> std::io::Result<String> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
        total += read as u64;
        on_bytes(total);
    }
    output.flush()?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::manifest::Manifest;

    /// A model with two tiny files, for tests that need one on disk.
    pub fn tiny_model_manifest() -> Manifest {
        let model = |id: &str, a: &str, b: &str| {
            let sha = |bytes: &[u8]| crate::download::sha256_bytes(bytes);
            serde_json::json!({
                "tier": "fast",
                "experimental": false,
                "displayName": id,
                "engine": "demo",
                "license": "MIT",
                "sources": [{"repo": "x/y", "revision": "0".repeat(40), "url": "https://example.com"}],
                "files": [
                    {"name": "a.bin", "dir": "sub", "role": "paddle-model", "urls": ["https://example.com/a"],
                     "sha256": sha(a.as_bytes()), "sizeBytes": a.len()},
                    {"name": "b.bin", "role": "paddle-model", "urls": ["https://example.com/b"],
                     "sha256": sha(b.as_bytes()), "sizeBytes": b.len()}
                ],
                "requirements": {"minVramMB": 0, "minRamMB": 0, "backends": ["cpu"]},
                "llamaServerArgs": [],
                "notes": ""
            })
        };
        let value = serde_json::json!({
            "schemaVersion": 1,
            "runtimes": {"llamaServer": {"build": "b1", "builds": {}}},
            "models": {"tiny": model("tiny", "alpha", "bravo!")},
            "suggester": {"defaultModel": "tiny", "recommend": [], "badges": []}
        });
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn install_state_follows_the_files_on_disk() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_installed(dir.path(), "tiny", model));

        let root = model_dir(dir.path(), "tiny");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("a.bin"), "alpha").unwrap();
        assert!(!is_installed(dir.path(), "tiny", model));
        fs::write(root.join("b.bin"), "wrong size").unwrap();
        assert!(!is_installed(dir.path(), "tiny", model));
        fs::write(root.join("b.bin"), "bravo!").unwrap();
        assert!(is_installed(dir.path(), "tiny", model));
        assert_eq!(installed_size(dir.path(), "tiny", model), Some(11));

        remove(dir.path(), "tiny").unwrap();
        assert!(!is_installed(dir.path(), "tiny", model));
        remove(dir.path(), "tiny").unwrap();
    }

    #[test]
    fn import_copies_verifies_and_finds_files_in_either_layout() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("sub")).unwrap();
        fs::write(source.path().join("sub").join("a.bin"), "alpha").unwrap();
        fs::write(source.path().join("b.bin"), "bravo!").unwrap();
        let models = tempfile::tempdir().unwrap();

        import_from_folder(models.path(), "tiny", model, source.path(), &|_, _| {}).unwrap();

        assert!(is_installed(models.path(), "tiny", model));
        assert!(!has_partial(models.path(), "tiny", model));
    }

    #[test]
    fn import_accepts_flat_folders() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.bin"), "alpha").unwrap();
        fs::write(source.path().join("b.bin"), "bravo!").unwrap();
        let models = tempfile::tempdir().unwrap();
        import_from_folder(models.path(), "tiny", model, source.path(), &|_, _| {}).unwrap();
        assert!(is_installed(models.path(), "tiny", model));
    }

    #[test]
    fn import_rejects_a_file_with_the_wrong_content() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.bin"), "alpha").unwrap();
        fs::write(source.path().join("b.bin"), "bravo?").unwrap();
        let models = tempfile::tempdir().unwrap();

        let error = import_from_folder(models.path(), "tiny", model, source.path(), &|_, _| {})
            .unwrap_err();

        assert_eq!(error.code, AppErrorCode::ChecksumMismatch);
        assert_eq!(error.detail.as_deref(), Some("b.bin"));
        assert!(!is_installed(models.path(), "tiny", model));
        assert!(!has_partial(models.path(), "tiny", model));
    }

    #[test]
    fn import_reports_a_missing_file_before_copying_anything() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.bin"), "alpha").unwrap();
        let models = tempfile::tempdir().unwrap();

        let error = import_from_folder(models.path(), "tiny", model, source.path(), &|_, _| {})
            .unwrap_err();

        assert_eq!(error.code, AppErrorCode::BadInput);
        assert_eq!(error.reason.as_deref(), Some("file_not_found"));
        assert!(!model_dir(models.path(), "tiny").exists());
    }

    #[test]
    fn import_reports_progress_per_file() {
        let manifest = tiny_model_manifest();
        let model = manifest.model("tiny").unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.bin"), "alpha").unwrap();
        fs::write(source.path().join("b.bin"), "bravo!").unwrap();
        let models = tempfile::tempdir().unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        import_from_folder(models.path(), "tiny", model, source.path(), &|i, n| {
            seen.lock().unwrap().push((i, n));
        })
        .unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![(0, 5), (1, 6)]);
    }
}
