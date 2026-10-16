//! The engine's Python environment (M1-8).
//!
//! The installer ships `uv` (an external binary) and the engine source (resources). On first run
//! the source is copied into the app data folder and `uv sync --locked --no-dev --python 3.12`
//! builds the environment there, with uv's Python installs and package cache kept inside app data
//! too. The environment is rebuilt when the bundled `uv.lock` changes; the source copy is
//! refreshed on every start so the engine code always matches the app.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::contract::{AppError, AppErrorCode};
use crate::download::sha256_bytes;
use crate::error::Result;
use crate::paths::AppPaths;
use crate::supervisor::job::Contained;

const STAMP: &str = ".synced";
const PYTHON_VERSION: &str = "3.12";

/// Files and folders of the engine project that belong in the environment's project folder.
const SOURCE_FILES: [&str; 3] = ["pyproject.toml", "uv.lock", ".python-version"];

#[derive(Debug, Clone)]
pub struct EngineEnv {
    /// `<app data>/engine`
    root: PathBuf,
    python_dir: PathBuf,
    uv_cache: PathBuf,
    /// The bundled engine project (`pyproject.toml`, `uv.lock`, `src/`).
    source: PathBuf,
    uv: PathBuf,
}

/// Where the bundled engine project is: `OPENDOC_ENGINE_SRC`, the app's resources, or (in debug
/// builds) the repository checkout the app was compiled from.
pub fn resolve_source(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENDOC_ENGINE_SRC") {
        return Some(PathBuf::from(path));
    }
    if let Some(dir) = resource_dir {
        let bundled = dir.join("engine");
        if bundled.join("pyproject.toml").is_file() {
            return Some(bundled);
        }
    }
    if cfg!(debug_assertions) {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine");
        if repo.join("pyproject.toml").is_file() {
            return Some(repo);
        }
    }
    None
}

/// The `uv` that ships with the app (next to the executable), `OPENDOC_UV`, or `uv` on the PATH
/// as a last resort for development builds.
pub fn resolve_uv() -> PathBuf {
    if let Ok(path) = std::env::var("OPENDOC_UV") {
        return PathBuf::from(path);
    }
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join(name);
            if std::fs::metadata(&bundled).is_ok_and(|m| m.len() > 0) {
                return bundled;
            }
        }
    }
    PathBuf::from("uv")
}

impl EngineEnv {
    pub fn new(paths: &AppPaths, source: PathBuf, uv: PathBuf) -> Self {
        Self {
            root: paths.engine.clone(),
            python_dir: paths.python.clone(),
            uv_cache: paths.uv_cache.clone(),
            source,
            uv,
        }
    }

    /// The project folder inside app data.
    pub fn project_dir(&self) -> PathBuf {
        self.root.join("app")
    }

    pub fn python(&self) -> PathBuf {
        let venv = self.project_dir().join(".venv");
        if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            venv.join("bin").join("python")
        }
    }

    fn bundled_lock_hash(&self) -> std::io::Result<String> {
        Ok(sha256_bytes(&std::fs::read(self.source.join("uv.lock"))?))
    }

    /// True when the environment was built from the bundled `uv.lock`.
    pub fn is_ready(&self) -> bool {
        let Ok(hash) = self.bundled_lock_hash() else {
            return false;
        };
        let stamp = std::fs::read_to_string(self.project_dir().join(STAMP)).unwrap_or_default();
        stamp.trim() == hash && self.python().is_file()
    }

    /// Copies the engine project into the app data folder. Cheap, so it runs on every start.
    pub fn refresh_source(&self) -> std::io::Result<()> {
        let project = self.project_dir();
        std::fs::create_dir_all(&project)?;
        for name in SOURCE_FILES {
            let from = self.source.join(name);
            if from.is_file() {
                std::fs::copy(&from, project.join(name))?;
            }
        }
        let target = project.join("src");
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        copy_tree(&self.source.join("src"), &target)
    }

    /// Makes the environment ready: refreshes the source and, when the lock changed or the
    /// environment is missing, runs `uv sync`. `on_line` gets uv's output lines (for the log).
    pub async fn ensure(&self, on_line: impl Fn(&str) + Send + Sync) -> Result<()> {
        if !self.source.join("pyproject.toml").is_file() {
            return Err(AppError::new(
                AppErrorCode::EngineUnavailable,
                "The engine source is missing from the app.",
            )
            .with_reason("engine_source_missing"));
        }
        let was_ready = self.is_ready();
        self.refresh_source()?;
        if was_ready {
            return Ok(());
        }
        self.sync(on_line).await?;
        let hash = self.bundled_lock_hash()?;
        std::fs::write(self.project_dir().join(STAMP), hash)?;
        Ok(())
    }

    fn uv_command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.uv);
        command
            .args([
                "sync",
                "--locked",
                "--no-dev",
                "--python",
                PYTHON_VERSION,
                "--no-progress",
            ])
            .current_dir(self.project_dir())
            .env("UV_PYTHON_INSTALL_DIR", &self.python_dir)
            .env("UV_CACHE_DIR", &self.uv_cache)
            .env("UV_PYTHON_PREFERENCE", "only-managed")
            .env("UV_NO_CONFIG", "1")
            .env_remove("VIRTUAL_ENV")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    async fn sync(&self, on_line: impl Fn(&str) + Send + Sync) -> Result<()> {
        let mut contained = Contained::spawn(self.uv_command()).map_err(|e| {
            AppError::new(AppErrorCode::EngineUnavailable, "uv could not be started.")
                .with_reason("uv_missing")
                .with_detail(format!("{}: {e}", self.uv.display()))
        })?;
        let stdout = contained.child.stdout.take();
        let stderr = contained.child.stderr.take();
        let mut tail: Vec<String> = Vec::new();

        let mut out_lines = stdout.map(|s| BufReader::new(s).lines());
        let mut err_lines = stderr.map(|s| BufReader::new(s).lines());
        loop {
            tokio::select! {
                line = async { out_lines.as_mut().unwrap().next_line().await }, if out_lines.is_some() => {
                    match line {
                        Ok(Some(line)) => { on_line(&line); push_tail(&mut tail, line); }
                        _ => out_lines = None,
                    }
                }
                line = async { err_lines.as_mut().unwrap().next_line().await }, if err_lines.is_some() => {
                    match line {
                        Ok(Some(line)) => { on_line(&line); push_tail(&mut tail, line); }
                        _ => err_lines = None,
                    }
                }
                else => break,
            }
        }
        let status = contained.child.wait().await?;
        if status.success() {
            return Ok(());
        }
        Err(classify_uv_failure(&tail.join("\n")))
    }
}

fn push_tail(tail: &mut Vec<String>, line: String) {
    if tail.len() >= 30 {
        tail.remove(0);
    }
    tail.push(line);
}

/// Turns uv's failure output into the app's error codes.
pub fn classify_uv_failure(output: &str) -> AppError {
    let lower = output.to_lowercase();
    let offline = [
        "error sending request",
        "dns error",
        "failed to lookup address",
        "connection refused",
        "network is unreachable",
        "timed out",
        "failed to fetch",
        "tcp connect error",
    ];
    let detail: String = output
        .chars()
        .rev()
        .take(1500)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if lower.contains("no space left")
        || lower.contains("not enough space")
        || lower.contains("os error 112")
    {
        AppError::new(AppErrorCode::DiskFull, "The disk is full.")
            .with_reason("disk_full")
            .with_detail(detail)
    } else if offline.iter().any(|needle| lower.contains(needle)) {
        AppError::new(
            AppErrorCode::Offline,
            "The engine's packages could not be downloaded.",
        )
        .with_reason("offline")
        .with_detail(detail)
    } else {
        AppError::new(
            AppErrorCode::DownloadFailed,
            "The engine environment could not be set up.",
        )
        .with_reason("engine_setup_failed")
        .with_detail(detail)
    }
}

/// Copies a folder tree, skipping bytecode caches.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "__pycache__" {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&source, &target)?;
        } else if name.to_string_lossy().ends_with(".pyc") {
            continue;
        } else {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_source(dir: &Path, lock: &str) -> PathBuf {
        let source = dir.join("bundle").join("engine");
        std::fs::create_dir_all(source.join("src").join("pkg").join("__pycache__")).unwrap();
        std::fs::write(source.join("pyproject.toml"), "[project]\nname='x'\n").unwrap();
        std::fs::write(source.join("uv.lock"), lock).unwrap();
        std::fs::write(source.join(".python-version"), "3.12\n").unwrap();
        std::fs::write(
            source.join("src").join("pkg").join("__init__.py"),
            "x = 1\n",
        )
        .unwrap();
        std::fs::write(
            source
                .join("src")
                .join("pkg")
                .join("__pycache__")
                .join("a.pyc"),
            "junk",
        )
        .unwrap();
        source
    }

    fn env_for(dir: &Path, source: PathBuf) -> EngineEnv {
        let paths = AppPaths::new(dir.join("data"), dir.join("logs"));
        EngineEnv::new(&paths, source, PathBuf::from("uv"))
    }

    #[test]
    fn refresh_copies_the_project_without_bytecode() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_for(dir.path(), engine_source(dir.path(), "lock-1"));
        env.refresh_source().unwrap();
        let project = env.project_dir();
        assert!(project.join("pyproject.toml").is_file());
        assert!(project.join("uv.lock").is_file());
        assert!(project.join(".python-version").is_file());
        assert!(project.join("src/pkg/__init__.py").is_file());
        assert!(!project.join("src/pkg/__pycache__").exists());
    }

    #[test]
    fn refresh_replaces_stale_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = engine_source(dir.path(), "lock-1");
        let env = env_for(dir.path(), source.clone());
        env.refresh_source().unwrap();
        std::fs::write(env.project_dir().join("src/pkg/old.py"), "stale").unwrap();
        env.refresh_source().unwrap();
        assert!(!env.project_dir().join("src/pkg/old.py").exists());
    }

    #[test]
    fn the_environment_is_ready_only_for_the_lock_it_was_built_from() {
        let dir = tempfile::tempdir().unwrap();
        let source = engine_source(dir.path(), "lock-1");
        let env = env_for(dir.path(), source.clone());
        assert!(!env.is_ready());

        env.refresh_source().unwrap();
        std::fs::create_dir_all(env.python().parent().unwrap()).unwrap();
        std::fs::write(env.python(), "").unwrap();
        std::fs::write(env.project_dir().join(STAMP), sha256_bytes(b"lock-1")).unwrap();
        assert!(env.is_ready());

        std::fs::write(source.join("uv.lock"), "lock-2").unwrap();
        assert!(!env.is_ready(), "a changed bundled lock needs a re-sync");
    }

    #[tokio::test]
    async fn a_missing_source_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_for(dir.path(), dir.path().join("nowhere"));
        let error = env.ensure(|_| {}).await.unwrap_err();
        assert_eq!(error.reason.as_deref(), Some("engine_source_missing"));
    }

    #[tokio::test]
    async fn a_missing_uv_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let source = engine_source(dir.path(), "lock-1");
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("logs"));
        let env = EngineEnv::new(&paths, source, dir.path().join("no-such-uv"));
        let error = env.ensure(|_| {}).await.unwrap_err();
        assert_eq!(error.reason.as_deref(), Some("uv_missing"));
    }

    #[test]
    fn uv_failures_are_classified() {
        let offline = classify_uv_failure(
            "error: Failed to fetch: https://pypi.org\n  error sending request",
        );
        assert_eq!(offline.code, AppErrorCode::Offline);
        let full =
            classify_uv_failure("error: failed to write: No space left on device (os error 28)");
        assert_eq!(full.code, AppErrorCode::DiskFull);
        let other = classify_uv_failure("error: lockfile needs to be updated");
        assert_eq!(other.code, AppErrorCode::DownloadFailed);
        assert_eq!(other.reason.as_deref(), Some("engine_setup_failed"));
    }

    #[test]
    fn the_repository_engine_is_found_in_debug_builds() {
        if std::env::var("OPENDOC_ENGINE_SRC").is_err() {
            let source = resolve_source(None).expect("repo engine");
            assert!(source.join("uv.lock").is_file());
        }
    }
}
