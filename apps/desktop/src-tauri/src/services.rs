//! Shared services every part of the core uses: paths, settings, the manifest, the hardware
//! probe, the downloader, the engine environment, the process supervisor and the event sink.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract::{AppError, AppErrorCode, Backend, HardwareInfo, Os};
use crate::download::Downloader;
use crate::engine_env::EngineEnv;
use crate::error::Result;
use crate::hardware::{self, RawProbe};
use crate::manifest::{self, Arch, Manifest, ModelEntry};
use crate::models;
use crate::paths::AppPaths;
use crate::runtime;
use crate::settings::SettingsStore;
use crate::supervisor::{LlamaLaunch, Supervisor};

/// Where events for the UI go. The app implements it with Tauri's `emit`; tests collect them.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

/// Drops events that arrive faster than `interval` for the same key, unless they are marked
/// important (a state change the UI must not miss).
pub struct Throttle {
    interval: Duration,
    last: Mutex<HashMap<String, Instant>>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, key: &str, important: bool) -> bool {
        let mut last = self.last.lock().expect("throttle lock");
        let now = Instant::now();
        let due = important
            || last
                .get(key)
                .is_none_or(|previous| now.duration_since(*previous) >= self.interval);
        if due {
            last.insert(key.to_string(), now);
        }
        due
    }

    pub fn forget(&self, key: &str) {
        self.last.lock().expect("throttle lock").remove(key);
    }
}

/// Seconds per page measured on this computer by `calibrate.run`, kept in a small JSON file.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CalibrationFile {
    models: HashMap<String, f64>,
    last: Option<String>,
}

pub struct CalibrationStore {
    file: PathBuf,
    data: Mutex<CalibrationFile>,
}

impl CalibrationStore {
    pub fn load(file: PathBuf) -> Self {
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            file,
            data: Mutex::new(data),
        }
    }

    pub fn all(&self) -> HashMap<String, f64> {
        self.data.lock().expect("calibration lock").models.clone()
    }

    pub fn last(&self) -> Option<(String, f64)> {
        let data = self.data.lock().expect("calibration lock");
        let id = data.last.clone()?;
        let seconds = *data.models.get(&id)?;
        Some((id, seconds))
    }

    pub fn get(&self, model_id: &str) -> Option<f64> {
        self.data
            .lock()
            .expect("calibration lock")
            .models
            .get(model_id)
            .copied()
    }

    pub fn set(&self, model_id: &str, seconds_per_page: f64) {
        let text = {
            let mut data = self.data.lock().expect("calibration lock");
            data.models.insert(model_id.to_string(), seconds_per_page);
            data.last = Some(model_id.to_string());
            serde_json::to_string_pretty(&*data).unwrap_or_default()
        };
        if let Some(parent) = self.file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(&self.file, text) {
            tracing::warn!("could not save the calibration: {error}");
        }
    }
}

/// The probe, cached for the life of the app (hardware does not change while it runs).
pub struct HardwareService {
    cached: tokio::sync::Mutex<Option<RawProbe>>,
}

impl HardwareService {
    pub fn new() -> Self {
        Self {
            cached: tokio::sync::Mutex::new(None),
        }
    }

    pub fn with_probe(probe: RawProbe) -> Self {
        Self {
            cached: tokio::sync::Mutex::new(Some(probe)),
        }
    }

    pub async fn raw(&self, refresh: bool) -> RawProbe {
        let mut cached = self.cached.lock().await;
        if refresh || cached.is_none() {
            let probe = tokio::task::spawn_blocking(hardware::probe_raw)
                .await
                .expect("the probe task does not panic");
            *cached = Some(probe);
        }
        cached.clone().expect("probe cached above")
    }
}

impl Default for HardwareService {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Services {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    pub manifest: Arc<Manifest>,
    pub hardware: HardwareService,
    pub downloader: Downloader,
    pub engine_env: EngineEnv,
    pub supervisor: Supervisor,
    pub calibration: CalibrationStore,
    /// The sample page `calibrate.run` times.
    pub calibration_page: PathBuf,
    pub sink: Arc<dyn EventSink>,
}

impl Services {
    pub fn models_dir(&self) -> PathBuf {
        PathBuf::from(self.settings.get().models_dir)
    }

    fn arch(os: Os) -> Arch {
        match os {
            Os::Windows => Arch::X64,
            Os::Macos => Arch::Arm64,
        }
    }

    /// The backend llama-server would use on this machine with the current settings.
    pub async fn backend(&self) -> Backend {
        let probe = self.hardware.raw(false).await;
        self.backend_for(&probe)
    }

    pub fn backend_for(&self, probe: &RawProbe) -> Backend {
        let required_cuda = self
            .manifest
            .llama_build(probe.os, Self::arch(probe.os), Backend::Cuda)
            .and_then(|(_, build)| manifest::cuda_version_of(build));
        hardware::select_backend(
            probe,
            required_cuda,
            self.settings.get().experimental_vulkan,
        )
    }

    pub async fn hardware_info(&self) -> HardwareInfo {
        let probe = self.hardware.raw(false).await;
        let backend = self.backend_for(&probe);
        let models_dir = self.models_dir();
        let dir = models_dir.clone();
        let free = tokio::task::spawn_blocking(move || hardware::free_disk_mb(&dir))
            .await
            .unwrap_or(0);
        hardware::to_info(&probe, backend, &models_dir, free)
    }

    pub fn model(&self, id: &str) -> Result<&ModelEntry> {
        self.manifest.model(id).ok_or_else(|| {
            AppError::invalid(format!("Unknown model: {id}")).with_reason("unknown_model")
        })
    }

    pub fn model_installed(&self, id: &str) -> bool {
        self.manifest
            .model(id)
            .is_some_and(|model| models::is_installed(&self.models_dir(), id, model))
    }

    /// Manifest key and extraction folder of the llama-server build for this machine.
    pub async fn runtime_location(&self) -> Option<(String, PathBuf)> {
        let probe = self.hardware.raw(false).await;
        let backend = self.backend_for(&probe);
        let (key, _) = self
            .manifest
            .llama_build(probe.os, Self::arch(probe.os), backend)?;
        let dir = runtime::build_dir(&self.paths.runtime, &self.manifest, &key);
        Some((key, dir))
    }

    /// How to start llama-server for a model: `None` for models that run inside the engine,
    /// `modelNotReady` when the model files or the runtime are not installed.
    pub async fn llama_launch(&self, model_id: &str) -> Result<Option<LlamaLaunch>> {
        let model = self.model(model_id)?;
        if !self.model_installed(model_id) {
            return Err(
                AppError::new(AppErrorCode::ModelNotReady, "The model is not installed.")
                    .with_reason("files_missing")
                    .with_detail(model_id.to_string()),
            );
        }
        if !model.needs_llama_server() {
            return Ok(None);
        }
        let backend = self.backend().await;
        let exe = match self.runtime_location().await {
            Some((_, dir)) => runtime::installed_server(&dir, &self.manifest),
            None => None,
        }
        .ok_or_else(|| {
            AppError::new(
                AppErrorCode::ModelNotReady,
                "The model runtime is not installed.",
            )
            .with_reason("runtime_missing")
        })?;
        Ok(Some(LlamaLaunch {
            exe,
            prefix_args: Vec::new(),
            env: Vec::new(),
            backend,
            models_dir: self.models_dir(),
            model_id: model_id.to_string(),
            model: model.clone(),
        }))
    }

    /// Starts (or keeps) what the model needs: llama-server for the GPU tiers, nothing for Fast.
    pub async fn prepare_model(&self, model_id: &str) -> Result<()> {
        let launch = self.llama_launch(model_id).await?;
        self.supervisor.ensure_model(launch).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_lets_important_events_through() {
        let throttle = Throttle::new(Duration::from_secs(60));
        assert!(throttle.allow("a", false));
        assert!(!throttle.allow("a", false));
        assert!(throttle.allow("a", true));
        assert!(throttle.allow("b", false));
    }

    #[test]
    fn throttle_reopens_after_the_interval() {
        let throttle = Throttle::new(Duration::from_millis(30));
        assert!(throttle.allow("a", false));
        std::thread::sleep(Duration::from_millis(50));
        assert!(throttle.allow("a", false));
    }

    #[test]
    fn calibration_is_stored_per_model_and_survives_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("calibration.json");
        let store = CalibrationStore::load(file.clone());
        assert!(store.last().is_none());
        store.set("accurate-paddleocr-vl-1.6", 4.8);
        store.set("fast-ppocrv6-tiny", 9.1);

        let again = CalibrationStore::load(file);
        assert_eq!(again.get("accurate-paddleocr-vl-1.6"), Some(4.8));
        assert_eq!(again.last(), Some(("fast-ppocrv6-tiny".to_string(), 9.1)));
        assert_eq!(again.all().len(), 2);
    }
}
