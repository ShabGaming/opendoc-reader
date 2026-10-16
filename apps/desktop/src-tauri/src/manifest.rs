//! Types for `models/manifest.json` (embedded at build time) and platform selection.
//!
//! The schema is `models/manifest.schema.json`; the engine tests validate the file against it.
//! This module only reads what the app needs.

use std::sync::OnceLock;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::contract::{Backend, FitBadge, Os, Tier};

const EMBEDDED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../models/manifest.json"
));

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub runtimes: Runtimes,
    pub models: IndexMap<String, ModelEntry>,
    pub suggester: Suggester,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtimes {
    pub llama_server: LlamaServerRuntime,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaServerRuntime {
    pub build: String,
    pub builds: IndexMap<String, RuntimeBuild>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBuild {
    pub backend: Backend,
    pub files: Vec<RuntimeFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileRole {
    Model,
    Mmproj,
    PaddleModel,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub name: String,
    pub dir: Option<String>,
    pub role: FileRole,
    pub urls: Vec<String>,
    pub sha256: String,
    pub size_bytes: u64,
}

impl ModelFile {
    /// Path of the file relative to the model's folder.
    pub fn relative_path(&self) -> std::path::PathBuf {
        match &self.dir {
            Some(dir) => std::path::Path::new(dir).join(&self.name),
            None => std::path::PathBuf::from(&self.name),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSource {
    pub repo: String,
    pub revision: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    #[serde(rename = "minVramMB")]
    pub min_vram_mb: u64,
    #[serde(rename = "minRamMB")]
    pub min_ram_mb: u64,
    pub backends: Vec<Backend>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub tier: Tier,
    pub experimental: bool,
    pub display_name: String,
    pub engine: String,
    pub license: String,
    pub sources: Vec<ModelSource>,
    pub files: Vec<ModelFile>,
    pub requirements: Requirements,
    pub llama_server_args: Vec<String>,
}

impl ModelEntry {
    pub fn download_size_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size_bytes).sum()
    }

    pub fn file_with_role(&self, role: FileRole) -> Option<&ModelFile> {
        self.files.iter().find(|file| file.role == role)
    }

    /// Whether a llama-server has to run for this model (the Fast tier runs in the engine only).
    pub fn needs_llama_server(&self) -> bool {
        self.file_with_role(FileRole::Model).is_some()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Range {
    pub min: Option<u64>,
    pub max: Option<u64>,
}

impl Range {
    pub fn contains(&self, value: u64) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }
}

/// Conditions on the probe. A rule matches when every condition present holds.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct When {
    pub backend: Option<Vec<Backend>>,
    #[serde(rename = "vramMB")]
    pub vram_mb: Option<Range>,
    #[serde(rename = "ramMB")]
    pub ram_mb: Option<Range>,
    #[serde(rename = "unifiedMemoryMB")]
    pub unified_memory_mb: Option<Range>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendRule {
    pub id: String,
    pub when: When,
    pub model: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeRule {
    pub id: String,
    pub models: Vec<String>,
    pub when: When,
    pub badge: FitBadge,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggester {
    pub default_model: String,
    pub recommend: Vec<RecommendRule>,
    pub badges: Vec<BadgeRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn current() -> Self {
        if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::X64
        }
    }
}

pub fn build_key(os: Os, arch: Arch, backend: Backend) -> String {
    let os = match os {
        Os::Windows => "windows",
        Os::Macos => "macos",
    };
    let arch = match arch {
        Arch::X64 => "x64",
        Arch::Arm64 => "arm64",
    };
    let backend = match backend {
        Backend::Cuda => "cuda",
        Backend::Vulkan => "vulkan",
        Backend::Metal => "metal",
        Backend::Cpu => "cpu",
    };
    format!("{os}-{arch}-{backend}")
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The manifest compiled into the app.
    pub fn embedded() -> &'static Manifest {
        static CELL: OnceLock<Manifest> = OnceLock::new();
        CELL.get_or_init(|| Manifest::parse(EMBEDDED).expect("the embedded manifest is valid"))
    }

    pub fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.get(id)
    }

    /// The llama-server build for this platform and backend, with its manifest key.
    pub fn llama_build(
        &self,
        os: Os,
        arch: Arch,
        backend: Backend,
    ) -> Option<(String, &RuntimeBuild)> {
        let key = build_key(os, arch, backend);
        self.runtimes
            .llama_server
            .builds
            .get(&key)
            .map(|build| (key, build))
    }
}

/// The CUDA toolkit version a build was made for, read from its file names
/// (`...cuda-13.4-x64.zip`), as `(major, minor)`.
pub fn cuda_version_of(build: &RuntimeBuild) -> Option<(u32, u32)> {
    build.files.iter().find_map(|file| {
        let rest = file.name.split("cuda-").nth(1)?;
        let version = rest.split('-').next()?;
        let (major, minor) = version.split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_manifest_parses() {
        let manifest = Manifest::embedded();
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.models.len() >= 4);
        assert!(manifest.runtimes.llama_server.build.starts_with('b'));
    }

    #[test]
    fn every_supported_platform_has_a_build() {
        let manifest = Manifest::embedded();
        for (os, arch, backend) in [
            (Os::Windows, Arch::X64, Backend::Cuda),
            (Os::Windows, Arch::X64, Backend::Vulkan),
            (Os::Windows, Arch::X64, Backend::Cpu),
            (Os::Macos, Arch::Arm64, Backend::Metal),
        ] {
            let (key, build) = manifest
                .llama_build(os, arch, backend)
                .unwrap_or_else(|| panic!("no build for {os:?} {backend:?}"));
            assert_eq!(build.backend, backend, "{key}");
        }
        assert!(manifest
            .llama_build(Os::Macos, Arch::Arm64, Backend::Cuda)
            .is_none());
    }

    #[test]
    fn suggester_rules_only_name_known_models() {
        let manifest = Manifest::embedded();
        let suggester = &manifest.suggester;
        assert!(manifest.model(&suggester.default_model).is_some());
        for rule in &suggester.recommend {
            assert!(manifest.model(&rule.model).is_some(), "{}", rule.id);
        }
        for rule in &suggester.badges {
            for model in &rule.models {
                assert!(manifest.model(model).is_some(), "{}", rule.id);
            }
        }
    }

    #[test]
    fn files_are_pinned_and_sized() {
        for (id, model) in &Manifest::embedded().models {
            assert!(!model.files.is_empty(), "{id}");
            for file in &model.files {
                assert_eq!(file.sha256.len(), 64, "{id}/{}", file.name);
                assert!(file.size_bytes > 0);
                assert!(!file.urls.is_empty());
            }
        }
    }

    #[test]
    fn only_the_fast_tier_runs_without_llama_server() {
        let manifest = Manifest::embedded();
        let fast = manifest.model("fast-ppocrv6-tiny").unwrap();
        assert!(!fast.needs_llama_server());
        assert!(manifest
            .model("accurate-paddleocr-vl-1.6")
            .unwrap()
            .needs_llama_server());
    }

    #[test]
    fn nested_model_files_keep_their_folder() {
        let fast = Manifest::embedded().model("fast-ppocrv6-tiny").unwrap();
        let first = &fast.files[0];
        assert_eq!(
            first.relative_path(),
            std::path::Path::new("PP-OCRv6_tiny_det").join("inference.json")
        );
    }

    #[test]
    fn the_cuda_version_is_read_from_the_file_name() {
        let manifest = Manifest::embedded();
        let (_, build) = manifest
            .llama_build(Os::Windows, Arch::X64, Backend::Cuda)
            .unwrap();
        assert_eq!(cuda_version_of(build), Some((13, 4)));
        let (_, cpu) = manifest
            .llama_build(Os::Windows, Arch::X64, Backend::Cpu)
            .unwrap();
        assert_eq!(cuda_version_of(cpu), None);
    }

    #[test]
    fn range_bounds_are_inclusive() {
        let range = Range {
            min: Some(8192),
            max: None,
        };
        assert!(range.contains(8192));
        assert!(!range.contains(8191));
        let capped = Range {
            min: None,
            max: Some(8191),
        };
        assert!(capped.contains(0));
        assert!(!capped.contains(8192));
    }
}
