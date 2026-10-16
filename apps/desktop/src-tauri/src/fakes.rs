//! Test helpers: the fake engine and fake llama-server (`tests/fixtures/*.py`), run with the
//! system Python, and the scratch folder that steers them.

use std::path::PathBuf;

use serde_json::Value;

use crate::contract::{AppError, Backend};
use crate::manifest::ModelEntry;
use crate::supervisor::{EngineSpawn, LlamaLaunch, SpawnFactory};

pub fn python() -> String {
    std::env::var("OPENDOC_TEST_PYTHON")
        .unwrap_or_else(|_| if cfg!(windows) { "python" } else { "python3" }.to_string())
}

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A scratch folder plus the environment variables that steer the fakes.
pub struct FakeEnv {
    pub dir: tempfile::TempDir,
    pub vars: Vec<(String, String)>,
}

impl Default for FakeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEnv {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("state")).unwrap();
        let vars = vec![
            (
                "FAKE_LOG".to_string(),
                dir.path().join("fake.log").to_string_lossy().into_owned(),
            ),
            (
                "FAKE_STATE_DIR".to_string(),
                dir.path().join("state").to_string_lossy().into_owned(),
            ),
        ];
        Self { dir, vars }
    }

    pub fn var(mut self, key: &str, value: &str) -> Self {
        self.vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn path(&self, name: &str) -> String {
        self.dir.path().join(name).to_string_lossy().into_owned()
    }
}

pub fn fake_engine_spawn(env: &FakeEnv) -> EngineSpawn {
    EngineSpawn {
        program: PathBuf::from(python()),
        args: vec![
            "-I".to_string(),
            fixture("fake_engine.py").to_string_lossy().into_owned(),
        ],
        cwd: None,
        env: env.vars.clone(),
    }
}

pub fn fake_engine_factory(env: &FakeEnv) -> SpawnFactory {
    let spawn = fake_engine_spawn(env);
    std::sync::Arc::new(move || -> Result<EngineSpawn, AppError> { Ok(spawn.clone()) })
}

/// Every request the fake engine has received so far, as `{method, params}`.
pub fn read_fake_log(env: &FakeEnv) -> Vec<Value> {
    std::fs::read_to_string(env.dir.path().join("fake.log"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("log line is JSON"))
        .collect()
}

/// A model entry whose weights are named like the real ones; the fake server ignores them.
pub fn fake_model(tier: &str) -> ModelEntry {
    serde_json::from_value(serde_json::json!({
        "tier": tier,
        "experimental": false,
        "displayName": "Fake",
        "engine": "demo",
        "license": "MIT",
        "sources": [{"repo": "x/y", "revision": "0".repeat(40), "url": "https://example.com"}],
        "files": [
            {"name": "m.gguf", "role": "model", "urls": ["https://example.com/m"], "sha256": "0".repeat(64), "sizeBytes": 1},
            {"name": "mm.gguf", "role": "mmproj", "urls": ["https://example.com/mm"], "sha256": "0".repeat(64), "sizeBytes": 1}
        ],
        "requirements": {"minVramMB": 0, "minRamMB": 0, "backends": ["cpu"]},
        "llamaServerArgs": ["--temp", "0"],
        "notes": ""
    }))
    .unwrap()
}

pub fn fake_llama_launch(env: &FakeEnv) -> LlamaLaunch {
    LlamaLaunch {
        exe: PathBuf::from(python()),
        prefix_args: vec![
            "-I".to_string(),
            fixture("fake_llama.py").to_string_lossy().into_owned(),
        ],
        env: env.vars.clone(),
        backend: Backend::Cpu,
        models_dir: env.dir.path().join("models"),
        model_id: "fake".to_string(),
        model: fake_model("accurate"),
    }
}
