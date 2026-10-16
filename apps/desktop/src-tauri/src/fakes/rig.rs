//! Whole-core fixtures: services over temp folders, the fake engine and a collecting event sink.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{fake_engine_factory, FakeEnv};
use crate::contract::{GpuInfo, GpuVendor, Os};
use crate::download::Downloader;
use crate::engine_env::EngineEnv;
use crate::hardware::RawProbe;
use crate::manifest::Manifest;
use crate::paths::AppPaths;
use crate::services::{CalibrationStore, EventSink, HardwareService, Services};
use crate::settings::SettingsStore;
use crate::supervisor::{Dirs, Supervisor, SupervisorConfig};

/// An event sink that remembers everything it was sent.
#[derive(Default)]
pub struct CollectSink {
    events: Mutex<Vec<(String, Value)>>,
}

impl CollectSink {
    pub fn events_named(&self, name: &str) -> Vec<(String, Value)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == name)
            .cloned()
            .collect()
    }
}

impl EventSink for CollectSink {
    fn emit(&self, event: &str, payload: Value) {
        self.events
            .lock()
            .unwrap()
            .push((event.to_string(), payload));
    }
}

/// A CPU-only Windows machine with 16 GB of RAM.
pub fn cpu_probe() -> RawProbe {
    RawProbe {
        os: Os::Windows,
        gpus: vec![GpuInfo {
            vendor: GpuVendor::Other,
            name: "Test adapter".into(),
            vram_mb: Some(0),
        }],
        nvidia_driver: None,
        cuda_driver_api: None,
        ram_mb: 16384,
        unified_memory_mb: None,
        cpu_name: "Test CPU".into(),
        cpu_cores: 8,
    }
}

/// The core's services over temp folders, the fake engine and the given manifest. The engine
/// environment counts as installed (so no `uv` is needed), and a sample page exists.
pub fn test_services(env: &FakeEnv, manifest: Manifest) -> (Arc<Services>, Arc<CollectSink>) {
    let root = env.dir.path();
    let paths = AppPaths::new(root.join("data"), root.join("logs"));
    paths.ensure().unwrap();

    let source = root.join("engine-bundle");
    std::fs::create_dir_all(source.join("src")).unwrap();
    std::fs::write(source.join("pyproject.toml"), "[project]\nname='x'\n").unwrap();
    std::fs::write(source.join("uv.lock"), "lock").unwrap();
    let engine_env = EngineEnv::new(&paths, source, PathBuf::from("uv-not-used"));
    engine_env.refresh_source().unwrap();
    std::fs::create_dir_all(engine_env.python().parent().unwrap()).unwrap();
    std::fs::write(engine_env.python(), "").unwrap();
    std::fs::write(
        engine_env.project_dir().join(".synced"),
        crate::download::sha256_bytes(b"lock"),
    )
    .unwrap();

    let settings = SettingsStore::load(paths.settings_file.clone(), &root.join("models"));
    let page = root.join("calibration-page.png");
    std::fs::write(&page, b"png").unwrap();
    let sink = Arc::new(CollectSink::default());
    let supervisor = Supervisor::new(
        fake_engine_factory(env),
        Dirs {
            work_dir: paths.work.clone(),
            models_dir: root.join("models"),
        },
        SupervisorConfig {
            stall: std::time::Duration::from_secs(30),
            health_timeout: std::time::Duration::from_secs(20),
            shutdown_grace: std::time::Duration::from_secs(2),
        },
    );
    let svc = Services {
        calibration: CalibrationStore::load(paths.calibration_file.clone()),
        paths,
        settings,
        manifest: Arc::new(manifest),
        hardware: HardwareService::with_probe(cpu_probe()),
        downloader: Downloader::new()
            .unwrap()
            .with_retry_delay(std::time::Duration::from_millis(5)),
        engine_env,
        supervisor,
        calibration_page: page,
        sink: sink.clone(),
    };
    (Arc::new(svc), sink)
}
