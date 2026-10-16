//! Tests against the real engine environment and the real llama-server.
//!
//! They need `uv`, network access (for the managed Python) and, for the llama-server tests, a
//! llama-server build and model files, so they only run when `OPENDOC_INTEGRATION=1`:
//!
//! ```text
//! OPENDOC_INTEGRATION=1 cargo test --test integration -- --nocapture
//! ```
//!
//! `OPENDOC_UV` names the uv to use (default: the sidecar in `binaries/`, else `uv` on the PATH).
//! `OPENDOC_LLAMA_DIR` and `OPENDOC_MODELS_DIR` point at a llama-server build folder and a
//! models folder laid out as `<modelsDir>/<modelId>/<file>`.

use std::path::PathBuf;
use std::time::Duration;

use opendoc_reader_lib::contract::Backend;
use opendoc_reader_lib::engine_env::EngineEnv;
use opendoc_reader_lib::manifest::Manifest;
use opendoc_reader_lib::paths::AppPaths;
use opendoc_reader_lib::supervisor::{EngineProcess, EngineSpawn, LlamaLaunch, LlamaServer};
use serde_json::{json, Value};

fn enabled() -> bool {
    if std::env::var("OPENDOC_INTEGRATION").as_deref() == Ok("1") {
        true
    } else {
        eprintln!("skipped: set OPENDOC_INTEGRATION=1 to run");
        false
    }
}

fn uv() -> PathBuf {
    if let Ok(path) = std::env::var("OPENDOC_UV") {
        return PathBuf::from(path);
    }
    let triple_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    if let Ok(entries) = std::fs::read_dir(&triple_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("uv-")
                && !name.ends_with(".stamp")
                && entry.metadata().is_ok_and(|m| m.len() > 0)
            {
                return entry.path();
            }
        }
    }
    PathBuf::from("uv")
}

#[tokio::test]
async fn the_engine_environment_is_built_by_uv_and_the_engine_answers() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path().join("data"), dir.path().join("logs"));
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine");
    let env = EngineEnv::new(&paths, source, uv());

    assert!(!env.is_ready());
    env.ensure(|line| eprintln!("uv: {line}")).await.unwrap();
    assert!(env.is_ready());
    // A second run changes nothing and is quick.
    env.ensure(|_| {}).await.unwrap();

    let spec = EngineSpawn {
        program: env.python(),
        args: vec!["-m".into(), "opendoc_engine".into()],
        cwd: Some(env.project_dir()),
        env: Vec::new(),
    };
    let engine = EngineProcess::spawn(&spec, Duration::from_secs(60)).unwrap();
    let ping = engine
        .request("engine.ping", Value::Null, None, false)
        .await
        .unwrap();
    assert_eq!(ping, json!("pong"));
    let version = engine
        .request("engine.version", Value::Null, None, false)
        .await
        .unwrap();
    assert_eq!(version["protocol"], "1");
    assert_eq!(version["irSchema"], "1");
    let capabilities = engine
        .request("engine.capabilities", Value::Null, None, false)
        .await
        .unwrap();
    assert!(capabilities.as_array().is_some_and(|list| !list.is_empty()));
    eprintln!(
        "engine {version}, {} engines",
        capabilities.as_array().unwrap().len()
    );

    // An unknown method comes back as a JSON-RPC error, not a crash.
    let unknown = engine
        .request("no.such.method", Value::Null, None, false)
        .await
        .unwrap_err();
    assert_eq!(unknown.code(), Some(-32601));

    engine.shutdown(Duration::from_secs(5)).await;
    assert!(!engine.is_alive());
}

async fn llama_round_trip(backend: Backend, build_dir: &str) {
    let llama_dir = PathBuf::from(
        std::env::var("OPENDOC_LLAMA_DIR")
            .unwrap_or_else(|_| format!(r"G:\opendoc-spike\bin\{build_dir}")),
    );
    let models_dir = PathBuf::from(
        std::env::var("OPENDOC_MODELS_DIR").unwrap_or_else(|_| r"G:\opendoc-spike\models".into()),
    );
    let exe = llama_dir.join(if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    });
    if !exe.is_file() || !models_dir.join("paddleocr-vl").is_dir() {
        eprintln!("skipped: no llama-server or models at the expected folders");
        return;
    }
    let launch = LlamaLaunch {
        exe,
        prefix_args: Vec::new(),
        env: Vec::new(),
        backend,
        models_dir,
        model_id: "paddleocr-vl".into(),
        model: Manifest::embedded()
            .model("accurate-paddleocr-vl-1.6")
            .unwrap()
            .clone(),
    };
    let mut server = LlamaServer::start(&launch, Duration::from_secs(240))
        .await
        .unwrap();
    let url = server.url();
    eprintln!("llama-server ({backend:?}) healthy at {url}");
    assert!(server.is_running());

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let props = client.get(format!("{url}/props")).send().await.unwrap();
    assert!(props.status().is_success());

    server.stop().await;
    assert!(!server.is_running());
    // The port is closed once the process is gone.
    let after = client
        .get(format!("{url}/health"))
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    assert!(after.is_err(), "llama-server still answers after stop");
}

#[tokio::test]
async fn llama_server_starts_on_the_cuda_build_and_stops() {
    if !enabled() {
        return;
    }
    llama_round_trip(Backend::Cuda, "llama-cuda").await;
}

#[tokio::test]
async fn llama_server_starts_on_the_cpu_build_and_stops() {
    if !enabled() {
        return;
    }
    llama_round_trip(Backend::Cpu, "llama-cpu").await;
}
