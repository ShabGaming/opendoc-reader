//! Tests against the real engine environment and the real llama-server.
//!
//! They need `uv`, network access (for the managed Python) and, for the llama-server tests, a
//! llama-server build and model files, so they only run when `OPENDOC_INTEGRATION=1`:
//!
//! ```text
//! OPENDOC_INTEGRATION=1 cargo test --test integration -- --nocapture
//! ```
//!
//! `OPENDOC_UV` names the uv to use (default: the real sidecar in `binaries/`, else `uv` on the
//! PATH). `tests/e2e.rs` is the whole-app run.
//! `OPENDOC_LLAMA_DIR` and `OPENDOC_MODELS_DIR` point at a llama-server build folder and a
//! models folder laid out as `<modelsDir>/<modelId>/<file>`.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{enabled, engine_source, uv};
use opendoc_reader_lib::contract::Backend;
use opendoc_reader_lib::engine_env::EngineEnv;
use opendoc_reader_lib::manifest::Manifest;
use opendoc_reader_lib::paths::AppPaths;
use opendoc_reader_lib::supervisor::{EngineProcess, EngineSpawn, LlamaLaunch, LlamaServer};
use serde_json::{json, Value};

#[tokio::test]
async fn the_engine_environment_is_built_by_uv_and_the_engine_answers() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path().join("data"), dir.path().join("logs"));
    let env = EngineEnv::new(&paths, engine_source(), uv());

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

    // The export path the queue uses, with the parameter shapes it sends: pages in, one
    // Document out, then Word and Markdown into free names.
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let image = work.join("page-0000.png");
    std::fs::write(&image, b"not really a png").unwrap();
    let ir = work.join("page-0000.json");
    std::fs::write(
        &ir,
        json!({
            "index": 0, "width": 100, "height": 100,
            "sourceImage": image.to_string_lossy(),
            "blocks": [
                {"id": "p0-b0", "type": "heading", "level": 1, "bbox": [1, 1, 90, 20], "order": 0, "text": "Title"},
                {"id": "p0-b1", "type": "paragraph", "bbox": [1, 30, 90, 50], "order": 1, "text": "Hello world"}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let document = work.join("document.json");
    engine
        .request(
            "document.assemble",
            json!({
                "source": {"path": dir.path().join("scan.pdf").to_string_lossy(), "kind": "pdf", "pageCount": 1},
                "engine": {"id": "paddleocr-vl", "model": "accurate-paddleocr-vl-1.6", "version": "0.1.0"},
                "pages": [ir.to_string_lossy()],
                "outPath": document.to_string_lossy(),
            }),
            None,
            true,
        )
        .await
        .unwrap();
    assert!(document.is_file());
    let docx = dir.path().join("scan.docx");
    engine
        .request(
            "export.docx",
            json!({"document": document.to_string_lossy(), "outputPath": docx.to_string_lossy()}),
            None,
            true,
        )
        .await
        .unwrap();
    assert!(docx.is_file());
    let markdown = dir.path().join("scan.md");
    engine
        .request(
            "export.markdown",
            json!({
                "document": document.to_string_lossy(),
                "outputPath": markdown.to_string_lossy(),
                "assetsDir": dir.path().join("scan_assets").to_string_lossy(),
            }),
            None,
            true,
        )
        .await
        .unwrap();
    let text = std::fs::read_to_string(&markdown).unwrap();
    assert!(
        text.contains("# Title") && text.contains("Hello world"),
        "{text}"
    );
    // The engine refuses to overwrite, which is why Rust picks free names.
    let again = engine
        .request(
            "export.docx",
            json!({"document": document.to_string_lossy(), "outputPath": docx.to_string_lossy()}),
            None,
            true,
        )
        .await
        .unwrap_err();
    assert_eq!(
        again.to_app_error().code,
        opendoc_reader_lib::contract::AppErrorCode::Io
    );
    assert_eq!(again.reason().as_deref(), Some("already_exists"));

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
