//! JSON-RPC client for the engine process (docs/protocol.md, Part 1).
//!
//! One [`EngineProcess`] is one running engine. It owns the child, writes requests to its stdin
//! and routes what comes back on stdout: responses to the request that is waiting for them and
//! `convert.progress` notifications to the progress callback of the call in flight. Stderr goes
//! to the app log. Serializing calls (the protocol allows one at a time) is the caller's job;
//! only `convert.cancel` and `engine.ping` may overlap with a running call.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::oneshot;

use super::job::Contained;
use crate::contract::{AppError, AppErrorCode};

pub const LLAMA_SERVER_UNREACHABLE: i64 = -32002;

/// How to start the engine.
#[derive(Debug, Clone)]
pub struct EngineSpawn {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum EngineError {
    /// The engine answered with a JSON-RPC error.
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    /// The process exited or closed stdout.
    Exited(String),
    /// Nothing (no response, no progress) for longer than the stall limit; the process was killed.
    Stalled,
    /// The engine broke the protocol or could not be started.
    Protocol(String),
}

impl EngineError {
    pub fn reason(&self) -> Option<String> {
        match self {
            EngineError::Rpc { data, .. } => data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str)
                .map(String::from),
            _ => None,
        }
    }

    pub fn code(&self) -> Option<i64> {
        match self {
            EngineError::Rpc { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// The mapping of docs/protocol.md section 2.5.
    pub fn to_app_error(&self) -> AppError {
        match self {
            EngineError::Rpc {
                code,
                message,
                data,
            } => {
                let reason = self.reason();
                let mut detail = message.clone();
                if let Some(extra) = data
                    .as_ref()
                    .and_then(|d| d.get("detail"))
                    .and_then(Value::as_str)
                {
                    detail = format!("{detail}: {extra}");
                }
                let (app_code, text) = match code {
                    -32001 => (AppErrorCode::ModelNotReady, "The model is not ready."),
                    -32002 => (
                        AppErrorCode::LlamaServerUnreachable,
                        "The model server did not answer.",
                    ),
                    -32003 => (AppErrorCode::OutOfMemory, "Not enough memory."),
                    -32004 => (AppErrorCode::BadInput, "The file cannot be converted."),
                    -32005 => (AppErrorCode::Cancelled, "Cancelled."),
                    -32008 => (
                        AppErrorCode::RecognitionFailed,
                        "The page could not be read.",
                    ),
                    -32009 if reason.as_deref() == Some("disk_full") => {
                        (AppErrorCode::DiskFull, "The disk is full.")
                    }
                    -32009 => (AppErrorCode::Io, "A file could not be written."),
                    _ => (AppErrorCode::Internal, "An internal error occurred."),
                };
                let mut error = AppError::new(app_code, text).with_detail(detail);
                error.reason = reason;
                error
            }
            EngineError::Exited(detail) => AppError::new(
                AppErrorCode::EngineUnavailable,
                "The document engine stopped.",
            )
            .with_reason("engine_exited")
            .with_detail(detail.clone()),
            EngineError::Stalled => AppError::new(
                AppErrorCode::EngineUnavailable,
                "The document engine stopped responding.",
            )
            .with_reason("engine_stalled"),
            EngineError::Protocol(detail) => AppError::new(
                AppErrorCode::EngineUnavailable,
                "The document engine could not be started.",
            )
            .with_reason("engine_protocol")
            .with_detail(detail.clone()),
        }
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_app_error())
    }
}

/// A `convert.progress` notification.
#[derive(Debug, Clone)]
pub struct Progress {
    pub page_index: u32,
    pub stage: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

type Pending = HashMap<i64, oneshot::Sender<Result<Value, EngineError>>>;

pub struct EngineProcess {
    contained: tokio::sync::Mutex<Contained>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pending: Mutex<Pending>,
    next_id: AtomicI64,
    last_activity: Mutex<Instant>,
    alive: AtomicBool,
    progress: Mutex<Option<ProgressFn>>,
    /// Id of the request that `convert.cancel` would stop; 0 when none.
    current: AtomicI64,
    stall: Duration,
    stderr_tail: Mutex<VecDeque<String>>,
    exit_note: Mutex<String>,
}

impl EngineProcess {
    pub fn spawn(spec: &EngineSpawn, stall: Duration) -> Result<Arc<Self>, EngineError> {
        let mut command = tokio::process::Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1");
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let mut contained = Contained::spawn(command).map_err(|e| {
            EngineError::Protocol(format!("could not start {}: {e}", spec.program.display()))
        })?;
        let child: &mut Child = &mut contained.child;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let process = Arc::new(Self {
            contained: tokio::sync::Mutex::new(contained),
            stdin: tokio::sync::Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            last_activity: Mutex::new(Instant::now()),
            alive: AtomicBool::new(true),
            progress: Mutex::new(None),
            current: AtomicI64::new(0),
            stall,
            stderr_tail: Mutex::new(VecDeque::new()),
            exit_note: Mutex::new(String::new()),
        });

        if let Some(stdout) = stdout {
            let reader = process.clone();
            tokio::spawn(async move { reader.read_stdout(stdout).await });
        }
        if let Some(stderr) = stderr {
            let logger = process.clone();
            tokio::spawn(async move { logger.read_stderr(stderr).await });
        }
        Ok(process)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// The last lines the engine wrote to stderr, for error details.
    pub fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .expect("stderr tail lock")
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn read_stdout(self: Arc<Self>, stdout: tokio::process::ChildStdout) {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            *self.last_activity.lock().expect("activity lock") = Instant::now();
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                tracing::warn!(target: "engine", "stdout line is not JSON");
                continue;
            };
            self.route(message);
        }
        self.alive.store(false, Ordering::SeqCst);
        let note = {
            let mut contained = self.contained.lock().await;
            match tokio::time::timeout(Duration::from_secs(2), contained.child.wait()).await {
                Ok(Ok(status)) => format!("exited with {status}"),
                _ => "closed its output".to_string(),
            }
        };
        let tail = self.stderr_tail();
        let note = if tail.is_empty() {
            note
        } else {
            format!("{note}; last output: {tail}")
        };
        *self.exit_note.lock().expect("exit note lock") = note.clone();
        let pending: Vec<_> = self.pending.lock().expect("pending lock").drain().collect();
        for (_, sender) in pending {
            let _ = sender.send(Err(EngineError::Exited(note.clone())));
        }
    }

    fn route(&self, message: Value) {
        if let Some(id) = message.get("id").and_then(Value::as_i64) {
            let result = if let Some(error) = message.get("error") {
                Err(EngineError::Rpc {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    data: error.get("data").cloned(),
                })
            } else {
                Ok(message.get("result").cloned().unwrap_or(Value::Null))
            };
            let sender = self.pending.lock().expect("pending lock").remove(&id);
            match sender {
                Some(sender) => {
                    let _ = sender.send(result);
                }
                None => tracing::warn!(target: "engine", "response for unknown request {id}"),
            }
            return;
        }
        if message.get("method").and_then(Value::as_str) == Some("convert.progress") {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let progress = Progress {
                page_index: params.get("pageIndex").and_then(Value::as_u64).unwrap_or(0) as u32,
                stage: params
                    .get("stage")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                current: params.get("current").and_then(Value::as_u64),
                total: params.get("total").and_then(Value::as_u64),
            };
            let handler = self.progress.lock().expect("progress lock").clone();
            if let Some(handler) = handler {
                handler(progress);
            }
        }
    }

    async fn read_stderr(self: Arc<Self>, stderr: tokio::process::ChildStderr) {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::info!(target: "engine", "{line}");
            let mut tail = self.stderr_tail.lock().expect("stderr tail lock");
            if tail.len() >= 40 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    }

    async fn write_line(&self, value: &Value) -> Result<(), EngineError> {
        let mut text = serde_json::to_string(value).expect("a Value serializes");
        text.push('\n');
        let mut guard = self.stdin.lock().await;
        let Some(stdin) = guard.as_mut() else {
            return Err(EngineError::Exited("stdin is closed".into()));
        };
        let write = async {
            stdin.write_all(text.as_bytes()).await?;
            stdin.flush().await
        };
        write
            .await
            .map_err(|e| EngineError::Exited(format!("could not write to the engine: {e}")))
    }

    /// Sends a request and waits for its response. `tracked` requests are the ones a
    /// `convert.cancel` can target and the ones whose progress is forwarded; ping and cancel
    /// are not tracked, so they can overlap with a call in flight.
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        progress: Option<ProgressFn>,
        tracked: bool,
    ) -> Result<Value, EngineError> {
        if !self.is_alive() {
            return Err(EngineError::Exited(
                self.exit_note.lock().expect("exit note lock").clone(),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, mut receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(id, sender);
        if tracked {
            *self.progress.lock().expect("progress lock") = progress;
            self.current.store(id, Ordering::SeqCst);
        }
        *self.last_activity.lock().expect("activity lock") = Instant::now();

        let mut message = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if !params.is_null() {
            message["params"] = params;
        }
        let outcome = match self.write_line(&message).await {
            Err(error) => Err(error),
            Ok(()) => self.wait_for(&mut receiver).await,
        };
        self.pending.lock().expect("pending lock").remove(&id);
        if tracked {
            *self.progress.lock().expect("progress lock") = None;
            self.current.store(0, Ordering::SeqCst);
        }
        outcome
    }

    async fn wait_for(
        &self,
        receiver: &mut oneshot::Receiver<Result<Value, EngineError>>,
    ) -> Result<Value, EngineError> {
        let check = (self.stall / 4).clamp(Duration::from_millis(20), Duration::from_secs(5));
        loop {
            tokio::select! {
                result = &mut *receiver => {
                    return result.unwrap_or_else(|_| Err(EngineError::Exited("the engine went away".into())));
                }
                _ = tokio::time::sleep(check) => {
                    let idle = self.last_activity.lock().expect("activity lock").elapsed();
                    if idle > self.stall {
                        tracing::warn!("the engine has been silent for {idle:?}; killing it");
                        self.kill().await;
                        return Err(EngineError::Stalled);
                    }
                }
            }
        }
    }

    /// Asks the engine to stop the call in flight, if there is one.
    pub async fn cancel_current(&self) {
        let target = self.current.load(Ordering::SeqCst);
        if target == 0 || !self.is_alive() {
            return;
        }
        let result = self
            .request("convert.cancel", json!({"requestId": target}), None, false)
            .await;
        if let Err(error) = result {
            tracing::warn!("convert.cancel failed: {error}");
        }
    }

    /// Closes stdin and waits `grace` for the engine to exit by itself, then kills it.
    pub async fn shutdown(&self, grace: Duration) {
        self.stdin.lock().await.take();
        let deadline = Instant::now() + grace;
        while self.is_alive() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        self.kill().await;
    }

    pub async fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.contained.lock().await.kill().await;
    }
}
