//! Chooses the app data folder and moves an existing install out of the old, identifier-based
//! folder (`<roaming app data>/io.github.shabgaming.opendocreader`) into `<local data>/OpenDoc
//! Reader`. Runs before logging starts, so progress goes to a note callback that the caller
//! writes to the log afterwards.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::DATA_FOLDER;

/// What happened to the old folder.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to move: no old data, or the new folder already has content.
    NotNeeded,
    Moved,
    /// The old folder is untouched and stays in use.
    Failed(String),
}

/// The data and log folders to use. `OPENDOC_APP_DATA` (passed as `override_dir`) wins and skips
/// migration. Otherwise the data lives in `<local_data>/OpenDoc Reader`, after moving the legacy
/// folder there if needed; when that move fails the legacy folders are used as they are.
pub fn locate(
    local_data: &Path,
    legacy_data: &Path,
    legacy_logs: &Path,
    override_dir: Option<&OsStr>,
    note: &mut dyn FnMut(&str),
) -> (PathBuf, PathBuf) {
    if let Some(dir) = override_dir {
        return (PathBuf::from(dir), PathBuf::from(dir).join("logs"));
    }
    let data = local_data.join(DATA_FOLDER);
    match migrate(legacy_data, &data, note) {
        Outcome::Failed(reason) => {
            note(&format!(
                "could not move {} to {}: {reason}; keeping the old location",
                legacy_data.display(),
                data.display()
            ));
            (legacy_data.to_path_buf(), legacy_logs.to_path_buf())
        }
        _ => (data.clone(), data.join("logs")),
    }
}

fn is_empty_dir(path: &Path) -> bool {
    match fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        // A missing folder is empty. Any other error (for example a file where the
        // folder should be, which is NotADirectory on Unix) is treated the same way,
        // so the move is attempted and its failure keeps the old location.
        Err(_) => true,
    }
}

/// True when the folder holds anything besides a log folder.
fn has_data(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name() != "logs")
    })
}

pub fn migrate(old: &Path, new: &Path, note: &mut dyn FnMut(&str)) -> Outcome {
    if old == new || !has_data(old) || !is_empty_dir(new) {
        return Outcome::NotNeeded;
    }
    note(&format!(
        "moving app data from {} to {}",
        old.display(),
        new.display()
    ));
    match move_folder(old, new, note) {
        Ok(()) => {
            note("app data moved");
            rebase_paths(old, new, note);
            Outcome::Moved
        }
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

fn move_folder(old: &Path, new: &Path, note: &mut dyn FnMut(&str)) -> io::Result<()> {
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    // Same volume: one rename, no copying.
    if new.exists() {
        fs::remove_dir(new)?;
    }
    match fs::rename(old, new) {
        Ok(()) => {
            note("renamed the folder");
            return Ok(());
        }
        Err(error) => note(&format!("rename not possible ({error}); copying instead")),
    }
    // Other volume, or a file is in use: copy to a staging folder, check it, then swap it in.
    let staging = new.with_file_name(format!("{DATA_FOLDER}.migrating"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    let result = copy_verified(old, &staging, note).and_then(|()| fs::rename(&staging, new));
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    // The copy is in place and verified; failing to clear the old folder loses nothing.
    if let Err(error) = fs::remove_dir_all(old) {
        note(&format!(
            "copied, but could not remove the old folder {}: {error}",
            old.display()
        ));
    }
    Ok(())
}

const MB: u64 = 1024 * 1024;

fn copy_verified(from: &Path, to: &Path, note: &mut dyn FnMut(&str)) -> io::Result<()> {
    let (files, bytes) = tree_size(from)?;
    note(&format!("copying {files} files, {} MB", bytes / MB));
    let mut copied = 0u64;
    let mut next_report = 512 * MB;
    copy_tree(from, to, &mut |n| {
        copied += n;
        if copied >= next_report {
            note(&format!("copied {} of {} MB", copied / MB, bytes / MB));
            next_report = copied + 512 * MB;
        }
    })?;
    if tree_size(to)? != (files, bytes) {
        return Err(io::Error::other("the copy does not match the original"));
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, progress)?;
        } else if kind.is_symlink() {
            copy_link(&entry.path(), &target)?;
        } else {
            progress(fs::copy(entry.path(), &target)?);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(from)?, to)
}

#[cfg(not(unix))]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to).map(|_| ())
}

/// Regular files and their total size.
fn tree_size(path: &Path) -> io::Result<(u64, u64)> {
    let (mut files, mut bytes) = (0, 0);
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            let (f, b) = tree_size(&entry.path())?;
            files += f;
            bytes += b;
        } else if !kind.is_symlink() {
            files += 1;
            bytes += entry.metadata()?.len();
        }
    }
    Ok((files, bytes))
}

/// Fixes what stored the old folder's absolute path or cannot survive a move. Failures are noted
/// and otherwise ignored: the moved data itself is intact.
fn rebase_paths(old: &Path, new: &Path, note: &mut dyn FnMut(&str)) {
    if let Err(error) = rebase_settings(old, new) {
        note(&format!("could not update settings.json: {error}"));
    }
    if let Err(error) = rebase_queue(old, new) {
        note(&format!("could not update the queue database: {error}"));
    }
    // The Python environment records absolute paths. Dropping its folder and stamp makes the
    // next setup rebuild it from the moved uv cache.
    let project = new.join("engine").join("app");
    let _ = fs::remove_dir_all(project.join(".venv"));
    let _ = fs::remove_file(project.join(".synced"));
}

fn rebase(value: &str, old: &Path, new: &Path) -> Option<String> {
    let rest = Path::new(value).strip_prefix(old).ok()?;
    Some(new.join(rest).to_string_lossy().into_owned())
}

/// A models folder the user chose elsewhere is left alone; one inside the old folder (the
/// default, once saved) follows the move.
fn rebase_settings(old: &Path, new: &Path) -> io::Result<()> {
    let file = new.join("settings.json");
    let Ok(text) = fs::read_to_string(&file) else {
        return Ok(());
    };
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(io::Error::other)?;
    let Some(dir) = json.get("modelsDir").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    if let Some(moved) = rebase(dir, old, new) {
        json["modelsDir"] = moved.into();
        fs::write(&file, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(())
}

fn rebase_queue(old: &Path, new: &Path) -> io::Result<()> {
    let file = new.join("queue.sqlite3");
    if !file.is_file() {
        return Ok(());
    }
    let db = rusqlite::Connection::open(&file).map_err(io::Error::other)?;
    let rows: Vec<(String, String)> = {
        let mut query = db
            .prepare("SELECT file_id, preview_path FROM files WHERE preview_path IS NOT NULL")
            .map_err(io::Error::other)?;
        let rows = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(io::Error::other)?;
        rows.filter_map(Result::ok).collect()
    };
    for (id, preview) in rows {
        if let Some(moved) = rebase(&preview, old, new) {
            db.execute(
                "UPDATE files SET preview_path = ?1 WHERE file_id = ?2",
                (moved, id),
            )
            .map_err(io::Error::other)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(root: &Path) -> PathBuf {
        let old = root
            .join("Roaming")
            .join("io.github.shabgaming.opendocreader");
        fs::create_dir_all(old.join("models").join("m1")).unwrap();
        fs::write(old.join("models").join("m1").join("model.gguf"), "weights").unwrap();
        fs::create_dir_all(old.join("logs")).unwrap();
        fs::write(old.join("logs").join("a.log"), "log").unwrap();
        old
    }

    #[test]
    fn override_wins_and_skips_migration() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        let custom = dir.path().join("dev");
        let (data, logs) = locate(
            &dir.path().join("Local"),
            &old,
            &old.join("logs"),
            Some(custom.as_os_str()),
            &mut |_| {},
        );
        assert_eq!(data, custom);
        assert_eq!(logs, custom.join("logs"));
        assert!(old.join("models").is_dir());
    }

    #[test]
    fn fresh_install_uses_the_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Roaming").join("old");
        let (data, logs) = locate(
            &dir.path().join("Local"),
            &old,
            &old.join("logs"),
            None,
            &mut |_| {},
        );
        assert_eq!(data, dir.path().join("Local").join("OpenDoc Reader"));
        assert_eq!(logs, data.join("logs"));
    }

    #[test]
    fn existing_install_is_renamed_into_place() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        let local = dir.path().join("Local");
        let mut log = Vec::new();
        let (data, _) = locate(&local, &old, &old.join("logs"), None, &mut |m| {
            log.push(m.to_string());
        });
        assert_eq!(data, local.join("OpenDoc Reader"));
        assert_eq!(
            fs::read_to_string(data.join("models/m1/model.gguf")).unwrap(),
            "weights"
        );
        assert!(!old.exists());
        assert!(log.iter().any(|m| m.contains("moving app data")));
    }

    #[test]
    fn new_folder_with_data_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        let new = dir.path().join("Local").join("OpenDoc Reader");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("settings.json"), "{}").unwrap();
        assert_eq!(migrate(&old, &new, &mut |_| {}), Outcome::NotNeeded);
        assert!(old.join("models").is_dir());
        assert!(!new.join("models").exists());
    }

    #[test]
    fn logs_only_old_folder_is_not_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        fs::create_dir_all(old.join("logs")).unwrap();
        let new = dir.path().join("new");
        assert_eq!(migrate(&old, &new, &mut |_| {}), Outcome::NotNeeded);
    }

    #[test]
    fn copy_fallback_reproduces_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        let staging = dir.path().join("staging");
        copy_verified(&old, &staging, &mut |_| {}).unwrap();
        assert_eq!(tree_size(&old).unwrap(), tree_size(&staging).unwrap());
        assert_eq!(
            fs::read_to_string(staging.join("models/m1/model.gguf")).unwrap(),
            "weights"
        );
    }

    #[test]
    fn failed_move_keeps_the_old_location() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        // The local data folder is a file, so nothing can be created inside it.
        let blocker = dir.path().join("Local");
        fs::write(&blocker, "x").unwrap();
        let mut log = Vec::new();
        let (data, logs) = locate(&blocker, &old, &old.join("logs"), None, &mut |m| {
            log.push(m.to_string());
        });
        assert_eq!(data, old);
        assert_eq!(logs, old.join("logs"));
        assert!(old.join("models/m1/model.gguf").is_file());
        assert!(log.iter().any(|m| m.contains("keeping the old location")));
    }

    #[test]
    fn moved_settings_and_queue_follow_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        fs::write(
            old.join("settings.json"),
            format!(
                "{{\"modelsDir\":{}}}",
                serde_json::to_string(&old.join("models")).unwrap()
            ),
        )
        .unwrap();
        let db = rusqlite::Connection::open(old.join("queue.sqlite3")).unwrap();
        db.execute_batch("CREATE TABLE files (file_id TEXT, preview_path TEXT)")
            .unwrap();
        let preview = old.join("work/f1/pages/page-0000.png");
        db.execute(
            "INSERT INTO files VALUES ('f1', ?1), ('f2', NULL)",
            [preview.to_string_lossy().as_ref()],
        )
        .unwrap();
        drop(db);
        fs::create_dir_all(old.join("engine/app/.venv")).unwrap();
        fs::write(old.join("engine/app/.synced"), "hash").unwrap();

        let new = dir.path().join("Local").join("OpenDoc Reader");
        assert_eq!(migrate(&old, &new, &mut |_| {}), Outcome::Moved);

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(new.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            Path::new(settings["modelsDir"].as_str().unwrap()),
            new.join("models")
        );
        let db = rusqlite::Connection::open(new.join("queue.sqlite3")).unwrap();
        let moved: String = db
            .query_row(
                "SELECT preview_path FROM files WHERE file_id='f1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(Path::new(&moved), new.join("work/f1/pages/page-0000.png"));
        assert!(!new.join("engine/app/.venv").exists());
        assert!(!new.join("engine/app/.synced").exists());
    }

    #[test]
    fn custom_models_folder_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy(dir.path());
        let new = dir.path().join("Local").join("OpenDoc Reader");
        let custom = dir.path().join("D-models");
        assert_eq!(rebase(custom.to_str().unwrap(), &old, &new), None);
    }
}
