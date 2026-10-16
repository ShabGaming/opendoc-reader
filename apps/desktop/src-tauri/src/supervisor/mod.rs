//! Process supervisor (M1-9): the engine process, the llama-server process, and the one place
//! that talks to them.
//!
//! `Supervisor::call` serializes engine requests (the protocol allows one at a time), starts the
//! engine on demand with the startup sequence of docs/protocol.md 1.2, and recovers from the two
//! failures that can be recovered from: an engine that died or stalled is restarted and the call
//! retried once, and a llama-server that stopped answering is restarted (same model, new port),
//! the engine reconfigured, and the call retried once.

pub mod job;
pub mod llama;
pub mod rpc;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::contract::AppError;
pub use llama::{LlamaLaunch, LlamaServer};
pub use rpc::{EngineError, EngineProcess, EngineSpawn, Progress, ProgressFn};

const PROTOCOL_VERSION: &str = "1";
const NOT_CONFIGURED: i64 = -32006;

pub type SpawnFactory = Arc<dyn Fn() -> Result<EngineSpawn, AppError> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// A call with no response and no progress for this long kills the engine.
    pub stall: Duration,
    /// How long llama-server may take to answer `/health` after it starts.
    pub health_timeout: Duration,
    /// How long `shutdown` waits for the engine to exit after closing its stdin.
    pub shutdown_grace: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            stall: Duration::from_secs(5 * 60),
            health_timeout: Duration::from_secs(5 * 60),
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dirs {
    pub work_dir: PathBuf,
    pub models_dir: PathBuf,
}

struct RunningLlama {
    server: LlamaServer,
    launch: LlamaLaunch,
}

#[derive(Default)]
struct Inner {
    engine: Option<Arc<EngineProcess>>,
    llama: Option<RunningLlama>,
    engine_version: Option<String>,
}

pub struct Supervisor {
    spawn: SpawnFactory,
    config: SupervisorConfig,
    dirs: Mutex<Dirs>,
    inner: tokio::sync::Mutex<Inner>,
    /// The running engine, readable without waiting for a call in flight (for cancel).
    live: Mutex<Option<Arc<EngineProcess>>>,
}

impl Supervisor {
    pub fn new(spawn: SpawnFactory, dirs: Dirs, config: SupervisorConfig) -> Self {
        Self {
            spawn,
            config,
            dirs: Mutex::new(dirs),
            inner: tokio::sync::Mutex::new(Inner::default()),
            live: Mutex::new(None),
        }
    }

    pub fn set_models_dir(&self, models_dir: PathBuf) {
        self.dirs.lock().expect("dirs lock").models_dir = models_dir;
    }

    pub fn dirs(&self) -> Dirs {
        self.dirs.lock().expect("dirs lock").clone()
    }

    /// The engine package version reported at startup, once the engine has been started.
    pub async fn engine_version(&self) -> Option<String> {
        self.inner.lock().await.engine_version.clone()
    }

    fn configure_params(&self, llama_url: Option<String>) -> Value {
        let dirs = self.dirs();
        json!({
            "llamaServerUrl": llama_url,
            "workDir": dirs.work_dir.to_string_lossy(),
            "modelsDir": dirs.models_dir.to_string_lossy(),
        })
    }

    async fn start_engine(&self, inner: &mut Inner) -> Result<Arc<EngineProcess>, EngineError> {
        if let Some(old) = inner.engine.take() {
            old.kill().await;
        }
        *self.live.lock().expect("live lock") = None;
        let spec = (self.spawn)().map_err(|e| {
            EngineError::Protocol(format!(
                "{}{}",
                e.message,
                e.detail.map(|d| format!(": {d}")).unwrap_or_default()
            ))
        })?;
        let engine = EngineProcess::spawn(&spec, self.config.stall)?;
        match self.handshake(&engine, inner).await {
            Ok(()) => {
                *self.live.lock().expect("live lock") = Some(engine.clone());
                inner.engine = Some(engine.clone());
                Ok(engine)
            }
            Err(error) => {
                engine.kill().await;
                Err(error)
            }
        }
    }

    /// `engine.ping` until it answers, `engine.version` (the protocol must be ours),
    /// `engine.capabilities`, `engine.configure`.
    async fn handshake(
        &self,
        engine: &Arc<EngineProcess>,
        inner: &mut Inner,
    ) -> Result<(), EngineError> {
        let ping = engine.request("engine.ping", Value::Null, None, false);
        match tokio::time::timeout(Duration::from_secs(120), ping).await {
            Ok(Ok(Value::String(pong))) if pong == "pong" => {}
            Ok(Ok(other)) => {
                return Err(EngineError::Protocol(format!(
                    "unexpected ping answer: {other}"
                )))
            }
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(EngineError::Protocol(
                    "the engine did not answer engine.ping".into(),
                ))
            }
        }
        let version = engine
            .request("engine.version", Value::Null, None, false)
            .await?;
        if version.get("protocol").and_then(Value::as_str) != Some(PROTOCOL_VERSION) {
            return Err(EngineError::Protocol(format!(
                "the engine speaks protocol {:?}, expected {PROTOCOL_VERSION:?}",
                version.get("protocol")
            )));
        }
        inner.engine_version = version
            .get("engine")
            .and_then(Value::as_str)
            .map(String::from);
        engine
            .request("engine.capabilities", Value::Null, None, false)
            .await?;
        let url = inner.llama.as_ref().map(|l| l.server.url());
        engine
            .request("engine.configure", self.configure_params(url), None, false)
            .await?;
        Ok(())
    }

    /// Sends the current llama-server URL to a running engine. A failure drops the engine so the
    /// next call starts a fresh, correctly configured one.
    async fn reconfigure(&self, inner: &mut Inner) {
        let Some(engine) = inner.engine.clone() else {
            return;
        };
        let url = inner.llama.as_ref().map(|l| l.server.url());
        let result = engine
            .request("engine.configure", self.configure_params(url), None, false)
            .await;
        if let Err(error) = result {
            tracing::warn!("engine.configure failed ({error}); the engine will be restarted");
            engine.kill().await;
            inner.engine = None;
            *self.live.lock().expect("live lock") = None;
        }
    }

    /// Calls an engine method. Returns the engine's `result`.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        progress: Option<ProgressFn>,
    ) -> Result<Value, EngineError> {
        let mut inner = self.inner.lock().await;
        let mut restarted_engine = false;
        let mut restarted_llama = false;
        let mut reconfigured = false;
        loop {
            let engine = match &inner.engine {
                Some(engine) if engine.is_alive() => engine.clone(),
                _ => match self.start_engine(&mut inner).await {
                    Ok(engine) => engine,
                    Err(error) if !restarted_engine => {
                        tracing::warn!("starting the engine failed ({error}); trying once more");
                        restarted_engine = true;
                        continue;
                    }
                    Err(error) => return Err(error),
                },
            };
            match engine
                .request(method, params.clone(), progress.clone(), true)
                .await
            {
                Ok(value) => return Ok(value),
                Err(error @ (EngineError::Exited(_) | EngineError::Stalled))
                    if !restarted_engine =>
                {
                    tracing::warn!("{method} lost the engine ({error}); restarting it");
                    restarted_engine = true;
                    engine.kill().await;
                    inner.engine = None;
                    *self.live.lock().expect("live lock") = None;
                }
                Err(error)
                    if error.code() == Some(rpc::LLAMA_SERVER_UNREACHABLE)
                        && !restarted_llama
                        && inner.llama.is_some() =>
                {
                    tracing::warn!("{method}: llama-server is unreachable; restarting it");
                    restarted_llama = true;
                    self.restart_llama(&mut inner).await.map_err(|e| {
                        EngineError::Protocol(format!("could not restart llama-server: {e}"))
                    })?;
                }
                Err(error) if error.code() == Some(NOT_CONFIGURED) && !reconfigured => {
                    reconfigured = true;
                    self.reconfigure(&mut inner).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Stops the call in flight (`convert.cancel`). Does not wait for any other call.
    pub async fn cancel_current(&self) {
        let engine = self.live.lock().expect("live lock").clone();
        if let Some(engine) = engine {
            engine.cancel_current().await;
        }
    }

    async fn restart_llama(&self, inner: &mut Inner) -> Result<(), AppError> {
        let Some(mut running) = inner.llama.take() else {
            return Ok(());
        };
        running.server.stop().await;
        let server = LlamaServer::start(&running.launch, self.config.health_timeout).await?;
        inner.llama = Some(RunningLlama {
            server,
            launch: running.launch,
        });
        self.reconfigure(inner).await;
        Ok(())
    }

    /// Makes sure llama-server is running the given model, or not running at all for `None`
    /// (models that run inside the engine). The engine is told the new URL.
    pub async fn ensure_model(&self, launch: Option<LlamaLaunch>) -> Result<(), AppError> {
        let mut inner = self.inner.lock().await;
        match launch {
            None => {
                if let Some(mut running) = inner.llama.take() {
                    running.server.stop().await;
                    self.reconfigure(&mut inner).await;
                }
                Ok(())
            }
            Some(launch) => {
                if let Some(running) = inner.llama.as_mut() {
                    if running.launch.model_id == launch.model_id
                        && running.launch.models_dir == launch.models_dir
                        && running.server.is_running()
                    {
                        return Ok(());
                    }
                }
                if let Some(mut running) = inner.llama.take() {
                    running.server.stop().await;
                }
                let server = LlamaServer::start(&launch, self.config.health_timeout).await?;
                inner.llama = Some(RunningLlama { server, launch });
                self.reconfigure(&mut inner).await;
                Ok(())
            }
        }
    }

    /// The model llama-server currently runs, if any.
    pub async fn running_model(&self) -> Option<String> {
        self.inner
            .lock()
            .await
            .llama
            .as_ref()
            .map(|l| l.launch.model_id.clone())
    }

    /// Stops llama-server and asks the engine to exit (stdin closed, killed after the grace
    /// period).
    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(engine) = inner.engine.take() {
            engine.shutdown(self.config.shutdown_grace).await;
        }
        *self.live.lock().expect("live lock") = None;
        if let Some(mut running) = inner.llama.take() {
            running.server.stop().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::{fake_engine_factory, fake_llama_launch, read_fake_log, FakeEnv};

    fn supervisor(env: &FakeEnv, stall_ms: u64) -> Supervisor {
        Supervisor::new(
            fake_engine_factory(env),
            Dirs {
                work_dir: env.dir.path().join("work"),
                models_dir: env.dir.path().join("models"),
            },
            SupervisorConfig {
                stall: Duration::from_millis(stall_ms),
                health_timeout: Duration::from_secs(20),
                shutdown_grace: Duration::from_secs(2),
            },
        )
    }

    fn methods(log: &[Value]) -> Vec<String> {
        log.iter()
            .map(|entry| entry["method"].as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn the_engine_starts_with_the_documented_sequence() {
        let env = FakeEnv::new();
        let supervisor = supervisor(&env, 30_000);
        let result = supervisor
            .call("pdf.inspect", json!({"path": "x.pdf"}), None)
            .await
            .unwrap();
        assert_eq!(result["pageCount"], 3);
        assert_eq!(supervisor.engine_version().await.as_deref(), Some("0.1.0"));

        let log = read_fake_log(&env);
        assert_eq!(
            methods(&log),
            vec![
                "engine.ping",
                "engine.version",
                "engine.capabilities",
                "engine.configure",
                "pdf.inspect"
            ]
        );
        let configure = &log[3]["params"];
        assert_eq!(configure["llamaServerUrl"], Value::Null);
        assert!(configure["workDir"].as_str().unwrap().ends_with("work"));
        assert!(configure["modelsDir"].as_str().unwrap().ends_with("models"));
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn progress_notifications_reach_the_callback() {
        let env = FakeEnv::new();
        let supervisor = supervisor(&env, 30_000);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let progress: ProgressFn = Arc::new(move |p| sink.lock().unwrap().push(p.stage));
        let params = json!({
            "engineId": "demo", "modelId": "m", "pageIndex": 0,
            "imagePath": env.path("page.png"), "irPath": env.path("ir.json"),
            "cropsDir": env.path("crops"),
        });
        supervisor
            .call("convert.page", params, Some(progress))
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["prepare".to_string()]);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn a_crashed_engine_is_restarted_and_the_call_retried_once() {
        let env = FakeEnv::new().var("FAKE_CRASH_ONCE_ON", "pdf.inspect");
        let supervisor = supervisor(&env, 30_000);
        let result = supervisor
            .call("pdf.inspect", json!({"path": "x.pdf"}), None)
            .await
            .unwrap();
        assert_eq!(result["pageCount"], 3);
        let log = read_fake_log(&env);
        let configures = methods(&log)
            .iter()
            .filter(|m| *m == "engine.configure")
            .count();
        assert_eq!(configures, 2, "configured again after the restart");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn an_engine_that_goes_silent_is_killed_and_reported() {
        let env = FakeEnv::new().var("FAKE_HANG_ON", "pdf.inspect");
        let supervisor = supervisor(&env, 600);
        let error = supervisor
            .call("pdf.inspect", json!({"path": "x.pdf"}), None)
            .await
            .unwrap_err();
        assert!(matches!(error, EngineError::Stalled), "{error:?}");
        let app = error.to_app_error();
        assert_eq!(app.code, crate::contract::AppErrorCode::EngineUnavailable);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn errors_from_the_engine_map_to_app_errors() {
        let env = FakeEnv::new()
            .var("FAKE_FAIL_PAGE", "0")
            .var("FAKE_FAIL_CODE", "-32003")
            .var("FAKE_FAIL_REASON", "gpu");
        let supervisor = supervisor(&env, 30_000);
        let params = json!({
            "engineId": "demo", "modelId": "m", "pageIndex": 0,
            "imagePath": env.path("page.png"), "irPath": env.path("ir.json"),
            "cropsDir": env.path("crops"),
        });
        let error = supervisor
            .call("convert.page", params, None)
            .await
            .unwrap_err()
            .to_app_error();
        assert_eq!(error.code, crate::contract::AppErrorCode::OutOfMemory);
        assert_eq!(error.reason.as_deref(), Some("gpu"));
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn cancel_stops_the_call_in_flight() {
        let env = FakeEnv::new().var("FAKE_SLOW_MS", "5000");
        let supervisor = Arc::new(supervisor(&env, 30_000));
        let params = json!({
            "engineId": "demo", "modelId": "m", "pageIndex": 0,
            "imagePath": env.path("page.png"), "irPath": env.path("ir.json"),
            "cropsDir": env.path("crops"),
        });
        let call = {
            let supervisor = supervisor.clone();
            tokio::spawn(async move { supervisor.call("convert.page", params, None).await })
        };
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let started = std::time::Instant::now();
        supervisor.cancel_current().await;
        let error = call.await.unwrap().unwrap_err();
        assert_eq!(error.code(), Some(-32005));
        assert!(started.elapsed() < Duration::from_secs(3));
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn llama_server_starts_waits_for_health_and_is_given_to_the_engine() {
        let env = FakeEnv::new().var("FAKE_LLAMA_LOADING_MS", "600");
        let supervisor = supervisor(&env, 30_000);
        supervisor
            .ensure_model(Some(fake_llama_launch(&env)))
            .await
            .unwrap();
        assert_eq!(supervisor.running_model().await.as_deref(), Some("fake"));
        supervisor
            .call("pdf.inspect", json!({"path": "x.pdf"}), None)
            .await
            .unwrap();
        let log = read_fake_log(&env);
        let configure = log
            .iter()
            .find(|e| e["method"] == "engine.configure")
            .unwrap();
        let url = configure["params"]["llamaServerUrl"].as_str().unwrap();
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");

        // Same model again: nothing restarts.
        supervisor
            .ensure_model(Some(fake_llama_launch(&env)))
            .await
            .unwrap();
        // The Fast tier needs none.
        supervisor.ensure_model(None).await.unwrap();
        assert_eq!(supervisor.running_model().await, None);
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn a_llama_server_that_dies_while_loading_is_an_error_with_its_output() {
        let env = FakeEnv::new().var("FAKE_LLAMA_EXIT", "3");
        let supervisor = supervisor(&env, 30_000);
        let error = supervisor
            .ensure_model(Some(fake_llama_launch(&env)))
            .await
            .unwrap_err();
        assert_eq!(error.code, crate::contract::AppErrorCode::ModelNotReady);
        assert_eq!(error.reason.as_deref(), Some("load_failed"));
        assert!(error.detail.unwrap().contains("failed to load model"));
    }

    #[tokio::test]
    async fn an_unreachable_llama_server_is_restarted_and_the_call_retried() {
        let env = FakeEnv::new().var("FAKE_LLAMA_UNREACHABLE", "1");
        let supervisor = supervisor(&env, 30_000);
        supervisor
            .ensure_model(Some(fake_llama_launch(&env)))
            .await
            .unwrap();
        let params = json!({
            "engineId": "demo", "modelId": "m", "pageIndex": 0,
            "imagePath": env.path("page.png"), "irPath": env.path("ir.json"),
            "cropsDir": env.path("crops"),
        });
        supervisor.call("convert.page", params, None).await.unwrap();

        let log = read_fake_log(&env);
        let urls: Vec<String> = log
            .iter()
            .filter(|e| e["method"] == "engine.configure")
            .map(|e| e["params"]["llamaServerUrl"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(urls.len(), 2);
        assert_ne!(urls[0], urls[1], "a new port after the restart");
        supervisor.shutdown().await;
    }

    #[tokio::test]
    async fn an_engine_that_cannot_be_started_is_reported() {
        let spawn: SpawnFactory = Arc::new(|| {
            Err(AppError::new(
                crate::contract::AppErrorCode::EngineUnavailable,
                "The engine is not installed.",
            ))
        });
        let supervisor = Supervisor::new(
            spawn,
            Dirs {
                work_dir: PathBuf::from("w"),
                models_dir: PathBuf::from("m"),
            },
            SupervisorConfig::default(),
        );
        let error = supervisor
            .call("engine.ping", Value::Null, None)
            .await
            .unwrap_err();
        assert!(matches!(error, EngineError::Protocol(_)));
    }
}
