//! One `llama-server` process for one model.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};

use super::job::Contained;
use crate::contract::{AppError, AppErrorCode, Backend};
use crate::manifest::{FileRole, ModelEntry};
use crate::models;

/// Everything needed to start (or restart) the server for a model.
#[derive(Debug, Clone)]
pub struct LlamaLaunch {
    pub exe: PathBuf,
    /// Arguments placed before the real ones; empty in the app, used by tests to run a script.
    pub prefix_args: Vec<String>,
    /// Extra environment for the process; empty in the app, used by tests.
    pub env: Vec<(String, String)>,
    pub backend: Backend,
    pub models_dir: PathBuf,
    pub model_id: String,
    pub model: ModelEntry,
}

pub struct LlamaServer {
    contained: Contained,
    pub port: u16,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

pub fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// The command line after the executable: `-m`, `--mmproj`, host and port, the GPU layer count
/// for the backend, then the model's own arguments from the manifest.
pub fn build_args(launch: &LlamaLaunch, port: u16) -> Result<Vec<String>, AppError> {
    let path_of = |role: FileRole| {
        launch.model.file_with_role(role).map(|file| {
            models::file_path(&launch.models_dir, &launch.model_id, file)
                .to_string_lossy()
                .into_owned()
        })
    };
    let model = path_of(FileRole::Model).ok_or_else(|| {
        AppError::new(
            AppErrorCode::ModelNotReady,
            "The model has no weights file.",
        )
        .with_reason("files_missing")
    })?;
    let mut args = launch.prefix_args.clone();
    args.extend(["-m".to_string(), model]);
    if let Some(mmproj) = path_of(FileRole::Mmproj) {
        args.extend(["--mmproj".to_string(), mmproj]);
    }
    args.extend([
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
    ]);
    args.extend([
        "-ngl".to_string(),
        match launch.backend {
            Backend::Cpu => "0".to_string(),
            _ => "99".to_string(),
        },
    ]);
    args.extend(launch.model.llama_server_args.iter().cloned());
    Ok(args)
}

impl LlamaServer {
    /// Starts the server and waits until `/health` answers 200 (it answers 503 while the model
    /// loads). Fails when the process exits first or `timeout` passes.
    pub async fn start(launch: &LlamaLaunch, timeout: Duration) -> Result<Self, AppError> {
        let port = free_port()?;
        let args = build_args(launch, port)?;
        let mut command = tokio::process::Command::new(&launch.exe);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        if let Some(dir) = launch
            .exe
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
        {
            command.current_dir(dir);
        }
        let mut contained = Contained::spawn(command).map_err(|e| {
            AppError::new(
                AppErrorCode::ModelNotReady,
                "llama-server could not be started.",
            )
            .with_reason("load_failed")
            .with_detail(e.to_string())
        })?;

        let tail = Arc::new(Mutex::new(VecDeque::new()));
        for stream in [
            contained.child.stdout.take().map(Stream::Out),
            contained.child.stderr.take().map(Stream::Err),
        ]
        .into_iter()
        .flatten()
        {
            let tail = tail.clone();
            tokio::spawn(async move { stream.pump(tail).await });
        }

        let mut server = Self {
            contained,
            port,
            stderr_tail: tail,
        };
        match server.wait_healthy(timeout).await {
            Ok(()) => Ok(server),
            Err(error) => {
                server.stop().await;
                Err(error)
            }
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn tail(&self) -> String {
        self.stderr_tail
            .lock()
            .expect("llama tail lock")
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.contained.child.try_wait(), Ok(None))
    }

    async fn wait_healthy(&mut self, timeout: Duration) -> Result<(), AppError> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| {
                AppError::internal("Could not create the HTTP client.").with_detail(e.to_string())
            })?;
        let url = format!("{}/health", self.url());
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.contained.child.try_wait() {
                return Err(AppError::new(
                    AppErrorCode::ModelNotReady,
                    "llama-server stopped while loading the model.",
                )
                .with_reason("load_failed")
                .with_detail(format!("{status}: {}", self.tail())));
            }
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(AppError::new(
                    AppErrorCode::ModelNotReady,
                    "llama-server did not become ready in time.",
                )
                .with_reason("load_failed")
                .with_detail(self.tail()));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn stop(&mut self) {
        self.contained.kill().await;
    }
}

enum Stream {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

impl Stream {
    async fn pump(self, tail: Arc<Mutex<VecDeque<String>>>) {
        async fn run<R: tokio::io::AsyncRead + Unpin>(
            reader: R,
            tail: Arc<Mutex<VecDeque<String>>>,
        ) {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "llama", "{line}");
                let mut tail = tail.lock().expect("llama tail lock");
                if tail.len() >= 30 {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
        match self {
            Stream::Out(reader) => run(reader, tail).await,
            Stream::Err(reader) => run(reader, tail).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    fn launch_for(id: &str, backend: Backend) -> LlamaLaunch {
        let manifest = Manifest::embedded();
        LlamaLaunch {
            exe: PathBuf::from("llama-server"),
            prefix_args: Vec::new(),
            env: Vec::new(),
            backend,
            models_dir: PathBuf::from("models"),
            model_id: id.to_string(),
            model: manifest.model(id).unwrap().clone(),
        }
    }

    #[test]
    fn arguments_follow_the_manifest_and_the_backend() {
        let launch = launch_for("accurate-paddleocr-vl-1.6", Backend::Cuda);
        let args = build_args(&launch, 5000).unwrap();
        let joined = args.join(" ");
        assert!(joined.contains("-m "));
        assert!(joined.contains("PaddleOCR-VL-1.6-GGUF.gguf"));
        assert!(joined.contains("--mmproj "));
        assert!(joined.contains("--host 127.0.0.1 --port 5000"));
        assert!(joined.contains("-ngl 99"));
        assert!(joined.ends_with("--temp 0 -np 1 -c 16384"));
    }

    #[test]
    fn the_cpu_backend_offloads_nothing() {
        let launch = launch_for("max-deepseek-ocr-1-q8", Backend::Cpu);
        let args = build_args(&launch, 5001).unwrap();
        let position = args.iter().position(|a| a == "-ngl").unwrap();
        assert_eq!(args[position + 1], "0");
        assert!(args.contains(&"--special".to_string()));
    }

    #[test]
    fn the_fast_tier_has_no_llama_server_command_line() {
        let launch = launch_for("fast-ppocrv6-tiny", Backend::Cpu);
        let error = build_args(&launch, 5002).unwrap_err();
        assert_eq!(error.code, AppErrorCode::ModelNotReady);
    }

    #[test]
    fn free_ports_are_not_zero() {
        assert!(free_port().unwrap() > 0);
    }
}
