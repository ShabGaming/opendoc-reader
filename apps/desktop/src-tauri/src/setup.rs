//! First-run setup and model installs (M1-3, M1-4, M1-7, M1-8, M1-11).
//!
//! One run installs what the chosen models need: the llama-server runtime for this machine's
//! backend (only when a model needs it), the engine environment, and the model files. Downloads
//! run side by side (bounded), progress is merged into one [`SetupStatus`] and emitted as
//! `setup://progress` at most about four times a second. After the downloads the first model is
//! timed with `calibrate.run`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::contract::{
    AppError, AppErrorCode, Backend, Calibration, ComponentKind, ComponentState, FitBadge,
    SetupComponent, SetupPhase, SetupStatus, EVENT_SETUP_PROGRESS,
};
use crate::download::{Control, DownloadFile, FileEvent};
use crate::error::Result;
use crate::manifest::{ModelEntry, ModelFile};
use crate::models;
use crate::runtime;
use crate::services::{Services, Throttle};
use crate::suggester;

const CONCURRENT_FILES: usize = 3;
const RUNTIME_ID: &str = "llama-server";
const ENGINE_ID: &str = "engine";

struct ComponentRuntime {
    component: SetupComponent,
    file_bytes: Vec<u64>,
    last_sample: Option<(Instant, u64)>,
}

struct State {
    phase: SetupPhase,
    components: Vec<ComponentRuntime>,
    calibration: Option<Calibration>,
    error: Option<AppError>,
    running: bool,
    control: Control,
}

pub struct SetupManager {
    svc: Arc<Services>,
    state: Mutex<State>,
    throttle: Throttle,
}

fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::Cuda => "CUDA",
        Backend::Vulkan => "Vulkan",
        Backend::Metal => "Metal",
        Backend::Cpu => "CPU",
    }
}

fn new_component(
    id: &str,
    kind: ComponentKind,
    name: String,
    file_count: usize,
    total: u64,
) -> ComponentRuntime {
    ComponentRuntime {
        component: SetupComponent {
            id: id.to_string(),
            kind,
            name,
            state: ComponentState::Pending,
            bytes_done: 0,
            bytes_total: total,
            bytes_per_second: None,
            seconds_left: None,
        },
        file_bytes: vec![0; file_count],
        last_sample: None,
    }
}

impl SetupManager {
    pub fn new(svc: Arc<Services>) -> Arc<Self> {
        Arc::new(Self {
            svc,
            state: Mutex::new(State {
                phase: SetupPhase::NotStarted,
                components: Vec::new(),
                calibration: None,
                error: None,
                running: false,
                control: Control::new(),
            }),
            throttle: Throttle::new(Duration::from_millis(250)),
        })
    }

    // -------------------------------------------------------------------------------------
    // Status

    fn snapshot(&self, state: &State) -> SetupStatus {
        SetupStatus {
            phase: state.phase,
            models_dir: self.svc.settings.get().models_dir,
            components: state
                .components
                .iter()
                .map(|c| c.component.clone())
                .collect(),
            calibration: state.calibration.clone(),
            error: state.error.clone(),
        }
    }

    /// The current status. During a run (and after a failed one) it comes from the run; at rest
    /// it is worked out from what is installed on disk, so it is right after a restart.
    pub async fn status(&self) -> SetupStatus {
        {
            let state = self.state.lock().expect("setup state lock");
            if state.running || state.phase == SetupPhase::Failed {
                return self.snapshot(&state);
            }
        }
        self.idle_status().await
    }

    async fn idle_status(&self) -> SetupStatus {
        let svc = &self.svc;
        let models_dir = svc.models_dir();
        let installed: Vec<(&String, &ModelEntry)> = svc
            .manifest
            .models
            .iter()
            .filter(|(id, model)| models::is_installed(&models_dir, id, model))
            .collect();
        let engine_ready = svc.engine_env.is_ready();
        let runtime_dir = svc.runtime_location().await.map(|(_, dir)| dir);
        let runtime_ok = runtime_dir
            .as_ref()
            .is_some_and(|dir| runtime::installed_server(dir, &svc.manifest).is_some());
        let needs_runtime = installed
            .iter()
            .any(|(_, model)| model.needs_llama_server());
        let ready = engine_ready && !installed.is_empty() && (!needs_runtime || runtime_ok);

        let done = |id: &str, kind: ComponentKind, name: String, size: u64| SetupComponent {
            id: id.to_string(),
            kind,
            name,
            state: ComponentState::Done,
            bytes_done: size,
            bytes_total: size,
            bytes_per_second: None,
            seconds_left: None,
        };
        let mut components = Vec::new();
        if runtime_ok {
            let backend = svc.backend().await;
            components.push(done(
                RUNTIME_ID,
                ComponentKind::Runtime,
                format!("llama-server ({})", backend_label(backend)),
                0,
            ));
        }
        if engine_ready {
            components.push(done(
                ENGINE_ID,
                ComponentKind::Engine,
                "Document engine".into(),
                0,
            ));
        }
        for (id, model) in &installed {
            components.push(done(
                id,
                ComponentKind::Model,
                model.display_name.clone(),
                model.download_size_bytes(),
            ));
        }
        let state = self.state.lock().expect("setup state lock");
        SetupStatus {
            phase: if ready {
                SetupPhase::Ready
            } else {
                SetupPhase::NotStarted
            },
            models_dir: svc.settings.get().models_dir,
            components,
            calibration: state.calibration.clone().or_else(|| {
                svc.calibration
                    .last()
                    .map(|(model_id, seconds_per_page)| Calibration {
                        model_id,
                        seconds_per_page,
                    })
            }),
            error: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.state.lock().expect("setup state lock").running
    }

    /// Model ids with a download in progress (for `models_list`).
    pub fn downloading_models(&self) -> Vec<String> {
        let state = self.state.lock().expect("setup state lock");
        if !state.running {
            return Vec::new();
        }
        state
            .components
            .iter()
            .filter(|c| c.component.kind == ComponentKind::Model)
            .map(|c| c.component.id.clone())
            .collect()
    }

    fn emit(&self, important: bool) {
        if !self.throttle.allow("setup", important) {
            return;
        }
        let snapshot = {
            let state = self.state.lock().expect("setup state lock");
            self.snapshot(&state)
        };
        match serde_json::to_value(&snapshot) {
            Ok(payload) => self.svc.sink.emit(EVENT_SETUP_PROGRESS, payload),
            Err(error) => tracing::warn!("could not serialize the setup status: {error}"),
        }
    }

    // -------------------------------------------------------------------------------------
    // Pause and resume

    pub fn pause(&self) {
        {
            let mut state = self.state.lock().expect("setup state lock");
            if !state.running || state.phase != SetupPhase::Downloading {
                return;
            }
            state.control.pause();
            state.phase = SetupPhase::Paused;
        }
        self.emit(true);
    }

    pub fn resume(&self) {
        {
            let mut state = self.state.lock().expect("setup state lock");
            if !state.running || state.phase != SetupPhase::Paused {
                return;
            }
            state.control.resume();
            state.phase = SetupPhase::Downloading;
        }
        self.emit(true);
    }

    // -------------------------------------------------------------------------------------
    // Starting runs

    /// Starts downloading what `model_ids` need. Returns once the run is accepted; calling it
    /// while a run is active resumes a paused one and otherwise does nothing.
    pub async fn start(self: &Arc<Self>, model_ids: Vec<String>) -> Result<()> {
        if model_ids.is_empty() {
            return Err(AppError::invalid("Choose at least one model.").with_reason("no_models"));
        }
        for id in &model_ids {
            self.svc.model(id)?;
        }
        if self.is_running() {
            self.resume();
            return Ok(());
        }
        self.begin_run(&model_ids, true).await?;
        let this = self.clone();
        tokio::spawn(async move { this.run(model_ids, true).await });
        Ok(())
    }

    /// `models_install`: like `start` for one model, rejected when it does not fit this machine
    /// or another run is active.
    pub async fn install(self: &Arc<Self>, model_id: String) -> Result<()> {
        self.svc.model(&model_id)?;
        let probe = self.svc.hardware.raw(false).await;
        let backend = self.svc.backend_for(&probe);
        let input = suggester::input_from(&probe, backend);
        let badge = suggester::badge_for(&self.svc.manifest, &model_id, &input);
        if matches!(badge, FitBadge::WontFit | FitBadge::Hidden) {
            return Err(AppError::invalid("This model does not fit this computer.")
                .with_reason("does_not_fit"));
        }
        if self.is_running() {
            return Err(AppError::invalid("A download is already running.")
                .with_reason("download_in_progress"));
        }
        let ids = vec![model_id];
        self.begin_run(&ids, false).await?;
        let this = self.clone();
        tokio::spawn(async move { this.run(ids, false).await });
        Ok(())
    }

    /// Builds the component list and flips to `downloading`.
    async fn begin_run(&self, model_ids: &[String], _calibrate: bool) -> Result<()> {
        let needs_runtime = model_ids.iter().any(|id| {
            self.svc
                .manifest
                .model(id)
                .is_some_and(ModelEntry::needs_llama_server)
        });
        let mut components = Vec::new();
        if needs_runtime {
            let backend = self.svc.backend().await;
            let files = self.runtime_files().await.unwrap_or_default();
            let total = files.iter().map(|f| f.size).sum();
            let count = files.len();
            components.push(new_component(
                RUNTIME_ID,
                ComponentKind::Runtime,
                format!("llama-server ({})", backend_label(backend)),
                count,
                total,
            ));
        }
        components.push(new_component(
            ENGINE_ID,
            ComponentKind::Engine,
            "Document engine".into(),
            1,
            0,
        ));
        for id in model_ids {
            let model = self.svc.model(id)?;
            components.push(new_component(
                id,
                ComponentKind::Model,
                model.display_name.clone(),
                model.files.len(),
                model.download_size_bytes(),
            ));
        }
        {
            let mut state = self.state.lock().expect("setup state lock");
            if state.running {
                return Err(AppError::invalid("A download is already running.")
                    .with_reason("download_in_progress"));
            }
            state.running = true;
            state.phase = SetupPhase::Downloading;
            state.components = components;
            state.error = None;
            state.control = Control::new();
        }
        self.emit(true);
        Ok(())
    }

    async fn runtime_files(&self) -> Result<Vec<DownloadFile>> {
        let probe = self.svc.hardware.raw(false).await;
        let backend = self.svc.backend_for(&probe);
        let arch = match probe.os {
            crate::contract::Os::Windows => crate::manifest::Arch::X64,
            crate::contract::Os::Macos => crate::manifest::Arch::Arm64,
        };
        let (_, build) = self
            .svc
            .manifest
            .llama_build(probe.os, arch, backend)
            .ok_or_else(|| {
                AppError::internal("No llama-server build exists for this computer.")
                    .with_reason("no_runtime_build")
            })?;
        let downloads = self.svc.paths.runtime.join("downloads");
        Ok(build
            .files
            .iter()
            .map(|file| DownloadFile {
                dest: downloads.join(&file.name),
                urls: vec![file.url.clone()],
                sha256: file.sha256.clone(),
                size: file.size_bytes,
            })
            .collect())
    }

    // -------------------------------------------------------------------------------------
    // The run

    async fn run(self: Arc<Self>, model_ids: Vec<String>, calibrate: bool) {
        let control = self.state.lock().expect("setup state lock").control.clone();
        let semaphore = Arc::new(Semaphore::new(CONCURRENT_FILES));
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();

        let needs_runtime = model_ids.iter().any(|id| {
            self.svc
                .manifest
                .model(id)
                .is_some_and(ModelEntry::needs_llama_server)
        });
        if needs_runtime {
            let this = self.clone();
            let (control, semaphore) = (control.clone(), semaphore.clone());
            tasks.spawn(async move { this.install_runtime(&control, &semaphore).await });
        }
        {
            let this = self.clone();
            tasks.spawn(async move { this.install_engine().await });
        }
        for id in &model_ids {
            let this = self.clone();
            let (control, semaphore, id) = (control.clone(), semaphore.clone(), id.clone());
            tasks.spawn(async move { this.install_model(&id, &control, &semaphore).await });
        }

        let mut failure: Option<AppError> = None;
        while let Some(joined) = tasks.join_next().await {
            let result = joined.unwrap_or_else(|e| {
                Err(AppError::internal("A setup task crashed.").with_detail(e.to_string()))
            });
            if let Err(error) = result {
                let replace = match &failure {
                    None => true,
                    Some(existing) => existing.code == AppErrorCode::Cancelled,
                };
                if replace {
                    failure = Some(error);
                }
                control.cancel();
            }
        }

        if let Some(error) = failure {
            tracing::warn!("setup failed: {error}");
            self.finish(SetupPhase::Failed, Some(error));
            return;
        }
        if calibrate {
            self.calibrate(&model_ids).await;
        }
        self.finish(SetupPhase::Ready, None);
    }

    fn finish(&self, phase: SetupPhase, error: Option<AppError>) {
        {
            let mut state = self.state.lock().expect("setup state lock");
            state.running = false;
            state.phase = phase;
            state.error = error;
            if phase == SetupPhase::Failed {
                for runtime in &mut state.components {
                    if runtime.component.state != ComponentState::Done {
                        runtime.component.state = ComponentState::Failed;
                    }
                }
            }
        }
        self.emit(true);
    }

    fn mark(&self, id: &str, new_state: ComponentState) {
        {
            let mut state = self.state.lock().expect("setup state lock");
            if let Some(runtime) = state.components.iter_mut().find(|c| c.component.id == id) {
                runtime.component.state = new_state;
                if new_state == ComponentState::Done {
                    runtime.component.bytes_per_second = None;
                    runtime.component.seconds_left = None;
                    if runtime.component.bytes_total > 0 {
                        runtime.component.bytes_done = runtime.component.bytes_total;
                    }
                }
            }
        }
        self.emit(true);
    }

    /// Records progress of one file of a component and updates its speed and time left.
    fn on_file_event(&self, id: &str, file_index: usize, event: FileEvent) {
        {
            let mut state = self.state.lock().expect("setup state lock");
            let Some(runtime) = state.components.iter_mut().find(|c| c.component.id == id) else {
                return;
            };
            match event {
                FileEvent::Bytes(bytes) => {
                    if let Some(slot) = runtime.file_bytes.get_mut(file_index) {
                        *slot = bytes;
                    }
                    let done: u64 = runtime.file_bytes.iter().sum();
                    runtime.component.bytes_done = done;
                    if runtime.component.state != ComponentState::Done {
                        runtime.component.state = ComponentState::Downloading;
                    }
                    let now = Instant::now();
                    match runtime.last_sample {
                        None => runtime.last_sample = Some((now, done)),
                        Some((then, before)) => {
                            let elapsed = now.duration_since(then).as_secs_f64();
                            if elapsed >= 0.5 {
                                let instant = done.saturating_sub(before) as f64 / elapsed;
                                let speed = match runtime.component.bytes_per_second {
                                    Some(previous) => previous * 0.7 + instant * 0.3,
                                    None => instant,
                                };
                                runtime.component.bytes_per_second = Some(speed);
                                runtime.component.seconds_left = (speed > 1.0
                                    && runtime.component.bytes_total > done)
                                    .then(|| (runtime.component.bytes_total - done) as f64 / speed);
                                runtime.last_sample = Some((now, done));
                            }
                        }
                    }
                }
                FileEvent::Verifying => runtime.component.state = ComponentState::Verifying,
            }
        }
        self.emit(matches!(event, FileEvent::Verifying));
    }

    async fn download_files(
        self: &Arc<Self>,
        id: &str,
        files: Vec<DownloadFile>,
        control: &Control,
        semaphore: &Arc<Semaphore>,
    ) -> Result<()> {
        let mut set: JoinSet<Result<()>> = JoinSet::new();
        for (index, file) in files.into_iter().enumerate() {
            let this = self.clone();
            let control = control.clone();
            let semaphore = semaphore.clone();
            let id = id.to_string();
            set.spawn(async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .map_err(|_| AppError::cancelled())?;
                let notify = |event: FileEvent| this.on_file_event(&id, index, event);
                this.svc
                    .downloader
                    .download(&file, &control, &notify)
                    .await?;
                this.on_file_event(&id, index, FileEvent::Bytes(file.size));
                Ok(())
            });
        }
        let mut failure: Option<AppError> = None;
        while let Some(joined) = set.join_next().await {
            let result = joined.unwrap_or_else(|e| {
                Err(AppError::internal("A download task crashed.").with_detail(e.to_string()))
            });
            if let Err(error) = result {
                if failure
                    .as_ref()
                    .is_none_or(|f| f.code == AppErrorCode::Cancelled)
                {
                    failure = Some(error);
                }
                set.abort_all();
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn install_model(
        self: &Arc<Self>,
        id: &str,
        control: &Control,
        semaphore: &Arc<Semaphore>,
    ) -> Result<()> {
        let model = self.svc.model(id)?.clone();
        let models_dir = self.svc.models_dir();
        if models::is_installed(&models_dir, id, &model) {
            self.mark(id, ComponentState::Done);
            return Ok(());
        }
        self.mark(id, ComponentState::Downloading);
        let files: Vec<DownloadFile> = model
            .files
            .iter()
            .map(|file| download_file_for(&models_dir, id, file))
            .collect();
        match self.download_files(id, files, control, semaphore).await {
            Ok(()) => {
                self.mark(id, ComponentState::Done);
                Ok(())
            }
            Err(error) => {
                self.mark(id, ComponentState::Failed);
                Err(error)
            }
        }
    }

    async fn install_runtime(
        self: &Arc<Self>,
        control: &Control,
        semaphore: &Arc<Semaphore>,
    ) -> Result<()> {
        let result = self.install_runtime_inner(control, semaphore).await;
        if result.is_err() {
            self.mark(RUNTIME_ID, ComponentState::Failed);
        }
        result
    }

    async fn install_runtime_inner(
        self: &Arc<Self>,
        control: &Control,
        semaphore: &Arc<Semaphore>,
    ) -> Result<()> {
        let (_, dir) = self.svc.runtime_location().await.ok_or_else(|| {
            AppError::internal("No llama-server build exists for this computer.")
                .with_reason("no_runtime_build")
        })?;
        if runtime::installed_server(&dir, &self.svc.manifest).is_some() {
            self.mark(RUNTIME_ID, ComponentState::Done);
            return Ok(());
        }
        self.mark(RUNTIME_ID, ComponentState::Downloading);
        let files = self.runtime_files().await?;
        let archives: Vec<PathBuf> = files.iter().map(|f| f.dest.clone()).collect();
        self.download_files(RUNTIME_ID, files, control, semaphore)
            .await?;

        self.mark(RUNTIME_ID, ComponentState::Verifying);
        let manifest = self.svc.manifest.clone();
        let target = dir.clone();
        let extracted = archives.clone();
        tokio::task::spawn_blocking(move || {
            runtime::install_archives(&extracted, &target, &manifest)
        })
        .await
        .map_err(|e| AppError::internal("Extraction crashed.").with_detail(e.to_string()))??;
        for archive in archives {
            let _ = std::fs::remove_file(archive);
        }
        self.mark(RUNTIME_ID, ComponentState::Done);
        Ok(())
    }

    async fn install_engine(self: &Arc<Self>) -> Result<()> {
        if self.svc.engine_env.is_ready() {
            // Still refresh the source so the engine matches this version of the app.
            let env = self.svc.engine_env.clone();
            let _ = tokio::task::spawn_blocking(move || env.refresh_source()).await;
            self.mark(ENGINE_ID, ComponentState::Done);
            return Ok(());
        }
        self.mark(ENGINE_ID, ComponentState::Downloading);
        let result = self
            .svc
            .engine_env
            .ensure(|line| tracing::debug!(target: "uv", "{line}"))
            .await;
        match result {
            Ok(()) => {
                self.mark(ENGINE_ID, ComponentState::Done);
                Ok(())
            }
            Err(error) => {
                self.mark(ENGINE_ID, ComponentState::Failed);
                Err(error)
            }
        }
    }

    // -------------------------------------------------------------------------------------
    // Calibration (M1-11)

    /// Times the first model that is not experimental with `calibrate.run` on the bundled
    /// sample page and keeps the result. A failure here never fails the setup: the speed
    /// estimates just stay estimates.
    async fn calibrate(&self, model_ids: &[String]) {
        let primary = model_ids
            .iter()
            .find(|id| self.svc.manifest.model(id).is_some_and(|m| !m.experimental))
            .or_else(|| model_ids.first());
        let Some(model_id) = primary else {
            return;
        };
        if !self.svc.calibration_page.is_file() {
            tracing::warn!("the calibration page is missing; skipping calibration");
            return;
        }
        {
            let mut state = self.state.lock().expect("setup state lock");
            state.phase = SetupPhase::Calibrating;
        }
        self.emit(true);
        match self.run_calibration(model_id).await {
            Ok(seconds) => {
                self.svc.calibration.set(model_id, seconds);
                let mut state = self.state.lock().expect("setup state lock");
                state.calibration = Some(Calibration {
                    model_id: model_id.clone(),
                    seconds_per_page: seconds,
                });
            }
            Err(error) => tracing::warn!("calibration failed: {error}"),
        }
        // Give the GPU back; the queue starts the server again when it needs it.
        let _ = self.svc.supervisor.ensure_model(None).await;
    }

    async fn run_calibration(&self, model_id: &str) -> Result<f64> {
        let model = self.svc.model(model_id)?;
        self.svc.prepare_model(model_id).await?;
        let result = self
            .svc
            .supervisor
            .call(
                "calibrate.run",
                json!({
                    "engineId": model.engine,
                    "modelId": model_id,
                    "samplePath": self.svc.calibration_page.to_string_lossy(),
                    "runs": 2,
                }),
                None,
            )
            .await
            .map_err(|e| e.to_app_error())?;
        result
            .get("secondsPerPage")
            .and_then(serde_json::Value::as_f64)
            .filter(|s| *s > 0.0)
            .ok_or_else(|| AppError::internal("calibrate.run returned no timing."))
    }

    // -------------------------------------------------------------------------------------
    // Import from a folder (M1-4)

    pub async fn import(self: &Arc<Self>, model_id: String, folder: String) -> Result<()> {
        let model = self.svc.model(&model_id)?.clone();
        let folder = PathBuf::from(folder);
        if !folder.is_dir() {
            return Err(
                AppError::new(AppErrorCode::BadInput, "The folder does not exist.")
                    .with_reason("file_not_found"),
            );
        }
        let component = new_component(
            &model_id,
            ComponentKind::Model,
            model.display_name.clone(),
            model.files.len(),
            model.download_size_bytes(),
        );
        {
            let mut state = self.state.lock().expect("setup state lock");
            if state.running {
                return Err(AppError::invalid("A download is already running.")
                    .with_reason("download_in_progress"));
            }
            state.running = true;
            state.phase = SetupPhase::Downloading;
            state.components = vec![component];
            state.error = None;
            state.control = Control::new();
        }
        self.emit(true);
        let this = self.clone();
        tokio::spawn(async move { this.run_import(model_id, model, folder).await });
        Ok(())
    }

    async fn run_import(self: Arc<Self>, model_id: String, model: ModelEntry, folder: PathBuf) {
        let models_dir = self.svc.models_dir();
        self.mark(&model_id, ComponentState::Verifying);
        let this = self.clone();
        let id = model_id.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let progress = |index: usize, bytes: u64| {
                this.on_file_event(&id, index, FileEvent::Bytes(bytes));
            };
            models::import_from_folder(&models_dir, &id, &model, &folder, &progress)
        })
        .await
        .unwrap_or_else(|e| {
            Err(AppError::internal("The import crashed.").with_detail(e.to_string()))
        });
        match outcome {
            Ok(()) => {
                self.mark(&model_id, ComponentState::Done);
                self.finish(SetupPhase::Ready, None);
            }
            Err(error) => {
                self.mark(&model_id, ComponentState::Failed);
                self.finish(SetupPhase::Failed, Some(error));
            }
        }
    }

    /// Clears a failed state so `status` reports from disk again (used when a model is removed
    /// or a new run is about to start).
    pub fn forget_failure(&self) {
        let mut state = self.state.lock().expect("setup state lock");
        if !state.running && state.phase == SetupPhase::Failed {
            state.phase = SetupPhase::NotStarted;
            state.components.clear();
            state.error = None;
        }
    }
}

fn download_file_for(models_dir: &std::path::Path, id: &str, file: &ModelFile) -> DownloadFile {
    DownloadFile {
        dest: models::file_path(models_dir, id, file),
        urls: file.urls.clone(),
        sha256: file.sha256.clone(),
        size: file.size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::{test_services, CollectSink, FakeEnv};
    use crate::testutil::{Route, TestServer};

    fn payloads() -> (Vec<u8>, Vec<u8>) {
        (
            b"alpha-file-contents".repeat(2000),
            b"bravo-file".repeat(1000),
        )
    }

    /// A manifest with one small model whose files are served by the test server.
    fn manifest_for(
        server: &TestServer,
        a: &[u8],
        b: &[u8],
        corrupt_b: bool,
    ) -> crate::manifest::Manifest {
        let sha = crate::download::sha256_bytes;
        let value = json!({
            "schemaVersion": 1,
            "runtimes": {"llamaServer": {"build": "b1", "builds": {}}},
            "models": {"tiny": {
                "tier": "fast", "experimental": false, "displayName": "Tiny", "engine": "demo",
                "license": "MIT",
                "sources": [{"repo": "x/y", "revision": "0".repeat(40), "url": "https://example.com"}],
                "files": [
                    {"name": "a.bin", "dir": "sub", "role": "paddle-model", "urls": [server.url("/a.bin")],
                     "sha256": sha(a), "sizeBytes": a.len()},
                    {"name": "b.bin", "role": "paddle-model", "urls": [server.url("/b.bin")],
                     "sha256": if corrupt_b { sha(b"other") } else { sha(b) }, "sizeBytes": b.len()}
                ],
                "requirements": {"minVramMB": 0, "minRamMB": 0, "backends": ["cpu"]},
                "llamaServerArgs": [], "notes": ""
            }},
            "suggester": {"defaultModel": "tiny", "recommend": [],
                          "badges": [{"id": "all", "models": ["tiny"], "when": {}, "badge": "runsWell"}]}
        });
        serde_json::from_value(value).unwrap()
    }

    fn phases(sink: &CollectSink) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for (_, payload) in sink.events_named(EVENT_SETUP_PROGRESS) {
            let phase = payload["phase"].as_str().unwrap().to_string();
            if seen.last() != Some(&phase) {
                seen.push(phase);
            }
        }
        seen
    }

    async fn wait_until_idle(setup: &SetupManager) {
        for _ in 0..600 {
            if !setup.is_running() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the setup run did not finish");
    }

    #[tokio::test]
    async fn a_full_run_downloads_calibrates_and_reports_ready() {
        let (a, b) = payloads();
        let server = TestServer::start(vec![
            ("/a.bin", Route::new(a.clone())),
            ("/b.bin", Route::new(b.clone())),
        ]);
        let env = FakeEnv::new();
        let (svc, sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let setup = SetupManager::new(svc.clone());

        assert_eq!(setup.status().await.phase, SetupPhase::NotStarted);
        setup.start(vec!["tiny".into()]).await.unwrap();
        wait_until_idle(&setup).await;

        let status = setup.status().await;
        assert_eq!(status.phase, SetupPhase::Ready);
        let model = status.components.iter().find(|c| c.id == "tiny").unwrap();
        assert_eq!(model.state, ComponentState::Done);
        assert_eq!(model.bytes_done, (a.len() + b.len()) as u64);
        assert!(models::is_installed(
            &svc.models_dir(),
            "tiny",
            svc.model("tiny").unwrap()
        ));

        let calibration = status.calibration.unwrap();
        assert_eq!(calibration.model_id, "tiny");
        assert_eq!(calibration.seconds_per_page, 0.5);
        assert_eq!(svc.calibration.get("tiny"), Some(0.5));

        let seen = phases(&sink);
        assert_eq!(seen.first().map(String::as_str), Some("downloading"));
        assert!(seen.contains(&"calibrating".to_string()), "{seen:?}");
        assert_eq!(seen.last().map(String::as_str), Some("ready"));
        // Every event is a full snapshot with the contract's fields.
        for (_, payload) in sink.events_named(EVENT_SETUP_PROGRESS) {
            assert!(payload.get("components").is_some() && payload.get("modelsDir").is_some());
        }
    }

    #[tokio::test]
    async fn setup_progress_is_throttled() {
        let (a, b) = payloads();
        let server = TestServer::start(vec![
            ("/a.bin", Route::new(a.clone())),
            ("/b.bin", Route::new(b.clone())),
        ]);
        let env = FakeEnv::new();
        let (svc, sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let setup = SetupManager::new(svc);
        let started = Instant::now();
        setup.start(vec!["tiny".into()]).await.unwrap();
        wait_until_idle(&setup).await;
        let elapsed = started.elapsed().as_secs_f64().max(1.0);
        let count = sink.events_named(EVENT_SETUP_PROGRESS).len() as f64;
        // State changes are always sent; byte progress at most ~4 per second.
        assert!(count < 40.0 + elapsed * 4.0, "{count} events in {elapsed}s");
    }

    #[tokio::test]
    async fn a_bad_checksum_fails_the_run_with_checksum_mismatch() {
        let (a, b) = payloads();
        let server = TestServer::start(vec![
            ("/a.bin", Route::new(a.clone())),
            ("/b.bin", Route::new(b.clone())),
        ]);
        let env = FakeEnv::new();
        let (svc, sink) = test_services(&env, manifest_for(&server, &a, &b, true));
        let setup = SetupManager::new(svc.clone());
        setup.start(vec!["tiny".into()]).await.unwrap();
        wait_until_idle(&setup).await;

        let status = setup.status().await;
        assert_eq!(status.phase, SetupPhase::Failed);
        assert_eq!(status.error.unwrap().code, AppErrorCode::ChecksumMismatch);
        assert_eq!(
            status
                .components
                .iter()
                .find(|c| c.id == "tiny")
                .unwrap()
                .state,
            ComponentState::Failed
        );
        assert!(!svc.model_installed("tiny"));
        assert_eq!(phases(&sink).last().map(String::as_str), Some("failed"));
    }

    #[tokio::test]
    async fn pause_holds_the_download_and_resume_finishes_it() {
        let (a, b) = (b"alpha".repeat(120_000), b"bravo".repeat(120_000));
        let server = TestServer::start(vec![
            (
                "/a.bin",
                Route::new(a.clone()).slow(Duration::from_millis(20)),
            ),
            (
                "/b.bin",
                Route::new(b.clone()).slow(Duration::from_millis(20)),
            ),
        ]);
        let env = FakeEnv::new();
        let (svc, _sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let setup = SetupManager::new(svc);
        setup.start(vec!["tiny".into()]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;

        setup.pause();
        assert_eq!(setup.status().await.phase, SetupPhase::Paused);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let held = setup.status().await;
        assert_eq!(held.phase, SetupPhase::Paused);
        let bytes_at_pause = held
            .components
            .iter()
            .find(|c| c.id == "tiny")
            .unwrap()
            .bytes_done;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let still = setup.status().await;
        assert_eq!(
            still
                .components
                .iter()
                .find(|c| c.id == "tiny")
                .unwrap()
                .bytes_done,
            bytes_at_pause
        );

        // Calling start again while paused resumes.
        setup.start(vec!["tiny".into()]).await.unwrap();
        wait_until_idle(&setup).await;
        assert_eq!(setup.status().await.phase, SetupPhase::Ready);
    }

    #[tokio::test]
    async fn import_installs_a_model_from_a_folder_and_reports_through_events() {
        let (a, b) = payloads();
        let server = TestServer::start(vec![]);
        let env = FakeEnv::new();
        let (svc, sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let source = env.dir.path().join("usb");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a.bin"), &a).unwrap();
        std::fs::write(source.join("b.bin"), &b).unwrap();
        let setup = SetupManager::new(svc.clone());

        setup
            .import("tiny".into(), source.to_string_lossy().into_owned())
            .await
            .unwrap();
        wait_until_idle(&setup).await;

        assert!(svc.model_installed("tiny"));
        assert_eq!(phases(&sink).last().map(String::as_str), Some("ready"));
    }

    #[tokio::test]
    async fn a_corrupt_import_reports_the_mismatch_through_the_event() {
        let (a, b) = payloads();
        let server = TestServer::start(vec![]);
        let env = FakeEnv::new();
        let (svc, _sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let source = env.dir.path().join("usb");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a.bin"), &a).unwrap();
        std::fs::write(source.join("b.bin"), b"not the right bytes").unwrap();
        let setup = SetupManager::new(svc.clone());

        setup
            .import("tiny".into(), source.to_string_lossy().into_owned())
            .await
            .unwrap();
        wait_until_idle(&setup).await;

        let status = setup.status().await;
        assert_eq!(status.phase, SetupPhase::Failed);
        assert_eq!(status.error.unwrap().code, AppErrorCode::ChecksumMismatch);
        assert!(!svc.model_installed("tiny"));
    }

    #[tokio::test]
    async fn unknown_models_and_empty_lists_are_rejected() {
        let server = TestServer::start(vec![]);
        let env = FakeEnv::new();
        let (a, b) = payloads();
        let (svc, _sink) = test_services(&env, manifest_for(&server, &a, &b, false));
        let setup = SetupManager::new(svc);
        assert_eq!(
            setup.start(vec![]).await.unwrap_err().code,
            AppErrorCode::InvalidArgument
        );
        assert_eq!(
            setup.start(vec!["nope".into()]).await.unwrap_err().code,
            AppErrorCode::InvalidArgument
        );
        assert_eq!(
            setup.install("nope".into()).await.unwrap_err().code,
            AppErrorCode::InvalidArgument
        );
    }
}
