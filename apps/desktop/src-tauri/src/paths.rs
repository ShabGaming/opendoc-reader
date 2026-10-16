//! Where the app keeps things on disk (docs/architecture.md, "Where data lives on disk").

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Per-OS app data directory from the Tauri path resolver.
    pub data: PathBuf,
    pub logs: PathBuf,
    /// One job directory per queued file (`<work>/<fileId>/`).
    pub work: PathBuf,
    /// Extracted llama-server builds.
    pub runtime: PathBuf,
    /// The engine source copy and its Python environment.
    pub engine: PathBuf,
    /// Interpreters and the package cache managed by uv.
    pub python: PathBuf,
    pub uv_cache: PathBuf,
    pub database: PathBuf,
    pub settings_file: PathBuf,
    pub calibration_file: PathBuf,
    /// Default models folder, used until the user picks another one.
    pub default_models: PathBuf,
}

impl AppPaths {
    pub fn new(data: PathBuf, logs: PathBuf) -> Self {
        Self {
            work: data.join("work"),
            runtime: data.join("runtime"),
            engine: data.join("engine"),
            python: data.join("python"),
            uv_cache: data.join("uv-cache"),
            database: data.join("queue.sqlite3"),
            settings_file: data.join("settings.json"),
            calibration_file: data.join("calibration.json"),
            default_models: data.join("models"),
            data,
            logs,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [&self.data, &self.logs, &self.work] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn job_dir(&self, file_id: &str) -> PathBuf {
        self.work.join(file_id)
    }

    pub fn page_image(&self, file_id: &str, page: u32) -> PathBuf {
        self.job_dir(file_id)
            .join("pages")
            .join(format!("page-{page:04}.png"))
    }

    pub fn page_ir(&self, file_id: &str, page: u32) -> PathBuf {
        self.job_dir(file_id)
            .join("ir")
            .join(format!("page-{page:04}.json"))
    }

    pub fn crops_dir(&self, file_id: &str) -> PathBuf {
        self.job_dir(file_id).join("crops")
    }

    pub fn document_json(&self, file_id: &str) -> PathBuf {
        self.job_dir(file_id).join("document.json")
    }
}

/// Native string form of a path for the contract (the UI treats it as opaque).
pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_layout_follows_the_protocol() {
        let paths = AppPaths::new(PathBuf::from("/data"), PathBuf::from("/logs"));
        let job = paths.job_dir("f-1");
        assert_eq!(paths.page_image("f-1", 3), job.join("pages/page-0003.png"));
        assert_eq!(paths.page_ir("f-1", 12), job.join("ir/page-0012.json"));
        assert_eq!(paths.crops_dir("f-1"), job.join("crops"));
        assert_eq!(paths.document_json("f-1"), job.join("document.json"));
    }
}
