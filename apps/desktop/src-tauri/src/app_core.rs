//! The core as the commands see it: the shared services plus the setup manager and the queue,
//! and the operations that cut across them.

use std::path::PathBuf;
use std::sync::Arc;

use crate::contract::{
    AppError, AppErrorCode, HardwareInfo, ModelInfo, ModelState, Recommendation, Settings,
    SettingsPatch,
};
use crate::download::Downloader;
use crate::engine_env::EngineEnv;
use crate::error::Result;
use crate::logging;
use crate::manifest::Manifest;
use crate::models;
use crate::paths::AppPaths;
use crate::queue::Queue;
use crate::runtime;
use crate::services::{CalibrationStore, EventSink, HardwareService, Services};
use crate::settings::SettingsStore;
use crate::setup::SetupManager;
use crate::suggester;
use crate::supervisor::{Dirs, EngineSpawn, SpawnFactory, Supervisor, SupervisorConfig};

pub struct Core {
    pub svc: Arc<Services>,
    pub setup: Arc<SetupManager>,
    pub queue: Arc<Queue>,
}

impl Core {
    /// Builds the real services for `paths` (the real engine, the embedded manifest) and the
    /// core on top of them. The app and the end-to-end test both start here; the queue's
    /// background loops and the log writer are left to the caller.
    pub fn assemble(
        paths: AppPaths,
        engine_source: PathBuf,
        uv: PathBuf,
        calibration_page: PathBuf,
        sink: Arc<dyn EventSink>,
    ) -> Result<Arc<Self>> {
        let engine_env = EngineEnv::new(&paths, engine_source, uv);
        let settings = SettingsStore::load(paths.settings_file.clone(), &paths.default_models);
        let models_dir = PathBuf::from(settings.get().models_dir);

        let spawn_env = engine_env.clone();
        let spawn: SpawnFactory = Arc::new(move || {
            if !spawn_env.is_ready() {
                return Err(AppError::new(
                    AppErrorCode::EngineUnavailable,
                    "The document engine is not installed yet.",
                )
                .with_reason("engine_not_installed"));
            }
            Ok(EngineSpawn {
                program: spawn_env.python(),
                args: vec!["-m".into(), "opendoc_engine".into()],
                cwd: Some(spawn_env.project_dir()),
                env: Vec::new(),
            })
        });
        let supervisor = Supervisor::new(
            spawn,
            Dirs {
                work_dir: paths.work.clone(),
                models_dir,
            },
            SupervisorConfig::default(),
        );
        let svc = Services {
            calibration: CalibrationStore::load(paths.calibration_file.clone()),
            settings,
            manifest: Arc::new(Manifest::embedded().clone()),
            hardware: HardwareService::new(),
            downloader: Downloader::new()?,
            engine_env,
            supervisor,
            calibration_page,
            sink,
            paths,
        };
        Ok(Arc::new(Core::new(Arc::new(svc))?))
    }

    pub fn new(svc: Arc<Services>) -> Result<Self> {
        let queue = Queue::new(svc.clone())?;
        let setup = SetupManager::new(svc.clone());
        Ok(Self { svc, setup, queue })
    }

    // -----------------------------------------------------------------------------------------
    // Hardware and models

    pub async fn probe_hardware(&self) -> HardwareInfo {
        self.svc.hardware.raw(true).await;
        self.svc.hardware_info().await
    }

    pub async fn recommendation(&self) -> Recommendation {
        let probe = self.svc.hardware.raw(false).await;
        let backend = self.svc.backend_for(&probe);
        let input = suggester::input_from(&probe, backend);
        suggester::build_recommendation(&self.svc.manifest, &input, &self.svc.calibration.all())
    }

    pub fn models_list(&self) -> Vec<ModelInfo> {
        let models_dir = self.svc.models_dir();
        let downloading = self.setup.downloading_models();
        self.svc
            .manifest
            .models
            .iter()
            .map(|(id, model)| ModelInfo {
                id: id.clone(),
                tier: model.tier,
                display_name: model.display_name.clone(),
                engine_id: model.engine.clone(),
                experimental: model.experimental,
                license: model.license.clone(),
                download_size_bytes: model.download_size_bytes(),
                installed_size_bytes: models::installed_size(&models_dir, id, model),
                state: models::state(&models_dir, id, model, downloading.contains(id)),
                source_url: model.sources.first().map(|s| s.url.clone()),
            })
            .collect()
    }

    pub async fn models_remove(&self, model_id: &str) -> Result<()> {
        self.svc.model(model_id)?;
        if self.queue.models_in_use().iter().any(|id| id == model_id)
            || self
                .setup
                .downloading_models()
                .iter()
                .any(|id| id == model_id)
        {
            return Err(AppError::invalid("This model is in use.").with_reason("model_in_use"));
        }
        // llama-server keeps the weights open; stop it before deleting them.
        if self.svc.supervisor.running_model().await.as_deref() == Some(model_id) {
            self.svc.supervisor.ensure_model(None).await?;
        }
        let dir = self.svc.models_dir();
        let id = model_id.to_string();
        tokio::task::spawn_blocking(move || models::remove(&dir, &id))
            .await
            .map_err(|e| AppError::internal("The removal crashed.").with_detail(e.to_string()))??;
        self.setup.forget_failure();
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Settings

    pub fn settings_set(&self, patch: SettingsPatch) -> Result<Settings> {
        let current = self.svc.settings.get();
        if let Some(model) = patch.default_model_id.as_deref().filter(|m| !m.is_empty()) {
            self.svc.model(model)?;
        }
        let new_dir = patch
            .models_dir
            .clone()
            .filter(|dir| !dir.is_empty() && *dir != current.models_dir);
        if let Some(dir) = &new_dir {
            let any_installed = self
                .svc
                .manifest
                .models
                .iter()
                .any(|(id, model)| models::is_installed(&self.svc.models_dir(), id, model));
            if any_installed || self.setup.is_running() {
                return Err(AppError::invalid(
                    "The models folder can only change while no model is installed or downloading.",
                )
                .with_reason("models_in_use"));
            }
            std::fs::create_dir_all(dir)?;
        }
        let next = self.svc.settings.patch(patch)?;
        if new_dir.is_some() {
            self.svc
                .supervisor
                .set_models_dir(PathBuf::from(&next.models_dir));
        }
        Ok(next)
    }

    // -----------------------------------------------------------------------------------------
    // Diagnostics

    /// The text behind "Copy diagnostics": versions, system, hardware, what is installed and the
    /// last log lines. It contains no file names and no document content; folders are left
    /// out, and the log lines have the home and app data folders replaced.
    pub async fn diagnostics(&self) -> Result<String> {
        use std::fmt::Write as _;
        let info = self.svc.hardware_info().await;
        let settings = self.svc.settings.get();
        let mut out = String::new();
        let _ = writeln!(out, "OpenDoc Reader {}", env!("CARGO_PKG_VERSION"));
        let os_name = sysinfo::System::long_os_version().unwrap_or_else(|| "unknown".into());
        let _ = writeln!(out, "OS: {os_name} ({:?})", info.os);
        let _ = writeln!(out, "CPU: {} ({} threads)", info.cpu_name, info.cpu_cores);
        let _ = writeln!(out, "Memory: {} MB", info.ram_mb);
        if let Some(unified) = info.unified_memory_mb {
            let _ = writeln!(out, "Unified memory: {unified} MB");
        }
        for gpu in &info.gpus {
            let vram = gpu
                .vram_mb
                .map(|v| format!("{v} MB"))
                .unwrap_or_else(|| "n/a".into());
            let _ = writeln!(out, "GPU: {} ({:?}, {vram})", gpu.name, gpu.vendor);
        }
        let _ = writeln!(out, "Backend: {:?}", info.backend);
        if let Some(driver) = &info.cuda_driver_version {
            let _ = writeln!(out, "NVIDIA driver: {driver}");
        }
        let _ = writeln!(
            out,
            "Free disk on the models drive: {} MB",
            info.free_disk_mb
        );

        let engine_version = self.svc.supervisor.engine_version().await;
        let _ = writeln!(
            out,
            "Engine: environment {}, version {}",
            if self.svc.engine_env.is_ready() {
                "ready"
            } else {
                "not installed"
            },
            engine_version.as_deref().unwrap_or("not started"),
        );
        let runtime_state = match self.svc.runtime_location().await {
            Some((key, dir)) => format!(
                "{key} {}",
                if runtime::installed_server(&dir, &self.svc.manifest).is_some() {
                    "installed"
                } else {
                    "not installed"
                }
            ),
            None => "none for this computer".into(),
        };
        let _ = writeln!(
            out,
            "llama-server: build {}, {runtime_state}",
            self.svc.manifest.runtimes.llama_server.build
        );
        let installed: Vec<String> = self
            .models_list()
            .into_iter()
            .filter(|m| m.state == ModelState::Installed)
            .map(|m| m.id)
            .collect();
        let _ = writeln!(
            out,
            "Installed models: {}",
            if installed.is_empty() {
                "none".into()
            } else {
                installed.join(", ")
            }
        );
        for (model, seconds) in self.svc.calibration.all() {
            let _ = writeln!(out, "Calibration: {model} {seconds:.1} s/page");
        }
        let _ = writeln!(
            out,
            "Settings: formats {:?}, theme {:?}, save to {}, default model {}, update check {}, Vulkan {}",
            settings.default_formats,
            settings.theme,
            match settings.save_to {
                crate::contract::SaveLocation::Original => "original",
                crate::contract::SaveLocation::Folder { .. } => "folder",
            },
            settings.default_model_id.as_deref().unwrap_or("recommended"),
            settings.check_for_updates,
            settings.experimental_vulkan,
        );

        let _ = writeln!(out, "\nRecent log:");
        let mut redact: Vec<(String, &str)> = vec![
            (path_of(&self.svc.paths.data), "<app data>"),
            (path_of(&self.svc.models_dir()), "<models>"),
        ];
        if let Some(home) = home_dir() {
            redact.push((home, "<home>"));
        }
        for line in logging::recent_lines(&self.svc.paths.logs, 80, &redact) {
            let _ = writeln!(out, "{line}");
        }
        Ok(out)
    }
}

fn path_of(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

fn home_dir() -> Option<String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .filter(|h| h.len() > 3)
}

/// Reject a path the OS file manager cannot show.
pub fn existing_path(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    if path.exists() {
        Ok(path)
    } else {
        Err(
            AppError::new(AppErrorCode::NotFound, "That path does not exist.")
                .with_reason("no_such_path"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Theme;
    use crate::fakes::{test_services, FakeEnv};
    use crate::models::tests::tiny_model_manifest;

    fn core() -> (Core, FakeEnv) {
        let env = FakeEnv::new();
        let (svc, _sink) = test_services(&env, tiny_model_manifest());
        (Core::new(svc).unwrap(), env)
    }

    fn install_tiny(core: &Core) {
        let dir = core.svc.models_dir().join("tiny");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("a.bin"), "alpha").unwrap();
        std::fs::write(dir.join("b.bin"), "bravo!").unwrap();
    }

    #[tokio::test]
    async fn models_list_reports_install_state_and_sizes() {
        let (core, _env) = core();
        let before = core.models_list();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].state, ModelState::NotInstalled);
        assert_eq!(before[0].installed_size_bytes, None);
        assert_eq!(before[0].download_size_bytes, 11);

        install_tiny(&core);
        let after = core.models_list();
        assert_eq!(after[0].state, ModelState::Installed);
        assert_eq!(after[0].installed_size_bytes, Some(11));
    }

    #[tokio::test]
    async fn removing_a_model_deletes_its_files_and_unknown_ids_are_rejected() {
        let (core, _env) = core();
        install_tiny(&core);
        core.models_remove("tiny").await.unwrap();
        assert!(!core.svc.model_installed("tiny"));
        assert_eq!(
            core.models_remove("nope").await.unwrap_err().code,
            AppErrorCode::InvalidArgument
        );
    }

    #[tokio::test]
    async fn settings_changes_apply_and_the_models_folder_is_protected() {
        let (core, env) = core();
        let next = core
            .settings_set(SettingsPatch {
                theme: Some(Theme::Dark),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(next.theme, Theme::Dark);

        let elsewhere = env
            .dir
            .path()
            .join("elsewhere")
            .to_string_lossy()
            .into_owned();
        let moved = core
            .settings_set(SettingsPatch {
                models_dir: Some(elsewhere.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(moved.models_dir, elsewhere);
        assert!(std::path::Path::new(&elsewhere).is_dir());

        install_tiny(&core);
        let error = core
            .settings_set(SettingsPatch {
                models_dir: Some(env.dir.path().join("third").to_string_lossy().into_owned()),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::InvalidArgument);
        assert_eq!(error.reason.as_deref(), Some("models_in_use"));

        let error = core
            .settings_set(SettingsPatch {
                default_model_id: Some("nope".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::InvalidArgument);
    }

    #[tokio::test]
    async fn the_vulkan_setting_switches_amd_and_intel_from_cpu_to_vulkan() {
        use crate::contract::{Backend, GpuInfo, GpuVendor};
        let (core, _env) = core();
        let mut probe = crate::fakes::cpu_probe();
        probe.gpus = vec![GpuInfo {
            vendor: GpuVendor::Amd,
            name: "AMD Radeon RX 7800 XT".into(),
            vram_mb: Some(16384),
        }];
        assert_eq!(core.svc.backend_for(&probe), Backend::Cpu);
        core.settings_set(SettingsPatch {
            experimental_vulkan: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(core.svc.backend_for(&probe), Backend::Vulkan);
    }

    #[tokio::test]
    async fn probe_hardware_reports_this_machine() {
        let (core, _env) = core();
        let info = core.probe_hardware().await;
        assert!(info.ram_mb > 0 && info.cpu_cores > 0);
        assert!(!info.models_dir.is_empty());
    }

    #[tokio::test]
    async fn diagnostics_has_no_paths_or_document_names() {
        let (core, env) = core();
        install_tiny(&core);
        // A log line that mentions a document must not survive redaction of the folders.
        let log_dir = &core.svc.paths.logs;
        std::fs::create_dir_all(log_dir).unwrap();
        let secret_folder = core.svc.paths.data.to_string_lossy().into_owned();
        std::fs::write(
            log_dir.join("opendoc-reader.2026-10-15.log"),
            format!("INFO engine: opened {secret_folder}/work/f-1\n"),
        )
        .unwrap();

        let text = core.diagnostics().await.unwrap();
        assert!(text.starts_with("OpenDoc Reader "));
        assert!(text.contains("Installed models: tiny"));
        assert!(text.contains("Backend: Cpu"));
        assert!(text.contains("<app data>/work/f-1"));
        assert!(!text.contains(&secret_folder));
        assert!(!text.contains(env.dir.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn existing_path_rejects_missing_paths() {
        assert_eq!(
            existing_path("/definitely/not/here").unwrap_err().code,
            AppErrorCode::NotFound
        );
    }
}
