//! Types of the command and event contract between the Rust core and the React UI.
//!
//! Types only: the commands themselves are implemented in `commands/` as the core lands. The
//! contract is written down in `docs/protocol.md` (Part 2), mirrored by hand in
//! `src/types/commands.ts`, and pinned by the JSON examples in `docs/protocol-examples.json`
//! (the test at the bottom of this file round-trips each of them).
//!
//! Wire conventions: camelCase keys, camelCase enum values, `None` serialized as `null` (never
//! omitted, except in [`SettingsPatch`]), and absolute native paths as strings.

use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------------------------------
// Errors

/// Rejection value of every command. The UI picks its copy from `code` and `reason`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: AppErrorCode,
    /// Short English text for logs; not meant to be shown as is.
    pub message: String,
    /// Narrows `code`; for `BadInput`: `file_not_found`, `unreadable`, `corrupt`,
    /// `password_protected`, `unsupported_format`, `page_out_of_range`.
    pub reason: Option<String>,
    /// Technical detail for the log and the diagnostics text.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppErrorCode {
    InvalidArgument,
    NotFound,
    BadInput,
    ModelNotReady,
    OutOfMemory,
    LlamaServerUnreachable,
    RecognitionFailed,
    EngineUnavailable,
    Cancelled,
    Offline,
    DownloadFailed,
    ChecksumMismatch,
    DiskFull,
    Io,
    Internal,
}

// -------------------------------------------------------------------------------------------
// Hardware, recommendation and models

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Fast,
    Accurate,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Cuda,
    Vulkan,
    Metal,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Os {
    Windows,
    Macos,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    pub name: String,
    /// Dedicated video memory in MB; `None` for Apple GPUs or when unknown.
    #[serde(rename = "vramMB")]
    pub vram_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub os: Os,
    pub gpus: Vec<GpuInfo>,
    pub backend: Backend,
    #[serde(rename = "ramMB")]
    pub ram_mb: u64,
    /// Apple Silicon unified memory in MB, `None` elsewhere.
    #[serde(rename = "unifiedMemoryMB")]
    pub unified_memory_mb: Option<u64>,
    pub cpu_name: String,
    pub cpu_cores: u32,
    /// The folder whose drive `free_disk_mb` was measured on.
    pub models_dir: String,
    #[serde(rename = "freeDiskMB")]
    pub free_disk_mb: u64,
    pub cuda_driver_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitBadge {
    RunsWell,
    Slow,
    WontFit,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeedBasis {
    Measured,
    Estimated,
    Calibrated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedEstimate {
    pub seconds_per_page: f64,
    pub basis: SpeedBasis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFit {
    pub model_id: String,
    pub badge: FitBadge,
    pub speed: Option<SpeedEstimate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub model_id: String,
    /// Plain-language reason with the manifest placeholders filled in.
    pub reason: String,
    pub fits: Vec<ModelFit>,
    /// Bytes for the llama-server build plus the engine environment (estimate).
    pub runtime_download_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelState {
    NotInstalled,
    Downloading,
    Installed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// The manifest model key.
    pub id: String,
    pub tier: Tier,
    pub display_name: String,
    /// The engine id that runs this model (the manifest `engine` field).
    pub engine_id: String,
    pub experimental: bool,
    pub license: String,
    pub download_size_bytes: u64,
    pub installed_size_bytes: Option<u64>,
    pub state: ModelState,
    pub source_url: Option<String>,
}

// -------------------------------------------------------------------------------------------
// Setup and downloads

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupPhase {
    NotStarted,
    Downloading,
    Paused,
    Calibrating,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    Runtime,
    Engine,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentState {
    Pending,
    Downloading,
    Verifying,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupComponent {
    pub id: String,
    pub kind: ComponentKind,
    pub name: String,
    pub state: ComponentState,
    pub bytes_done: u64,
    /// 0 when the total is unknown.
    pub bytes_total: u64,
    pub bytes_per_second: Option<f64>,
    pub seconds_left: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    pub model_id: String,
    pub seconds_per_page: f64,
}

/// Command result of `setup_status` and payload of the `setup://progress` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub phase: SetupPhase,
    pub models_dir: String,
    pub components: Vec<SetupComponent>,
    pub calibration: Option<Calibration>,
    pub error: Option<AppError>,
}

// -------------------------------------------------------------------------------------------
// Settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Docx,
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveLocation {
    Original,
    Folder { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub default_formats: Vec<ExportFormat>,
    /// `None` means "use the recommendation".
    pub default_model_id: Option<String>,
    pub save_to: SaveLocation,
    pub theme: Theme,
    pub check_for_updates: bool,
    pub models_dir: String,
    pub experimental_vulkan: bool,
}

/// Argument of `settings_set`. A missing field and a `null` both mean "leave as is".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_formats: Option<Vec<ExportFormat>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_to: Option<SaveLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_for_updates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental_vulkan: Option<bool>,
}

// -------------------------------------------------------------------------------------------
// Queue

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Pdf,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueStatus {
    Waiting,
    Converting,
    Paused,
    NeedsAttention,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttentionReason {
    HasTextLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMode {
    Ocr,
    ExistingText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Render,
    Prepare,
    Layout,
    Recognize,
    Normalize,
    Crops,
    Export,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputFile {
    pub format: ExportFormat,
    pub path: String,
    pub assets_dir: Option<String>,
}

/// One saved output of a queue item and whether the file is still there (`check_outputs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputStatus {
    pub format: ExportFormat,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub file_id: String,
    pub path: String,
    pub name: String,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub has_text_layer: Option<bool>,
    pub preview_path: Option<String>,
    pub status: QueueStatus,
    pub attention: Option<AttentionReason>,
    pub text_mode: Option<TextMode>,
    pub model_id: Option<String>,
    pub formats: Vec<ExportFormat>,
    pub pages_done: u32,
    pub current_page: Option<u32>,
    pub stage: Option<Stage>,
    pub seconds_left: Option<f64>,
    pub duration_ms: Option<u64>,
    pub outputs: Vec<OutputFile>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageState {
    Pending,
    Converting,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageStatus {
    pub index: u32,
    pub state: PageState,
    pub image_path: Option<String>,
    pub error: Option<AppError>,
}

/// Argument of `queue_start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    pub formats: Vec<ExportFormat>,
    pub model_id: String,
    pub save_to: SaveLocation,
}

/// Payload of the `queue://progress` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueProgress {
    pub item: QueueItem,
    pub page: Option<PageStatus>,
}

/// The engine's description of which engine and model produced a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub model: String,
    pub version: String,
}

/// Result of `get_review`. `page` is the IR page (see `engine/schema/ir.schema.json`) passed
/// through from the job's page file; the Rust core does not interpret it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPage {
    pub file_id: String,
    pub page_index: u32,
    pub page_count: u32,
    pub image_path: String,
    pub page: serde_json::Value,
    pub engine: EngineInfo,
}

// -------------------------------------------------------------------------------------------
// Event names

pub const EVENT_QUEUE_PROGRESS: &str = "queue://progress";
pub const EVENT_SETUP_PROGRESS: &str = "setup://progress";

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    /// The shared examples that `docs/protocol.md` quotes and the UI tests check.
    fn examples() -> serde_json::Map<String, Value> {
        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/protocol-examples.json"
        ));
        match serde_json::from_str(text).expect("protocol-examples.json is valid JSON") {
            Value::Object(map) => map,
            other => panic!("protocol-examples.json must be an object, got {other}"),
        }
    }

    /// Deserializes the example into `T` and checks that serializing it back gives exactly the
    /// same JSON. This fails on a renamed field, a wrong enum value, a missing field or a field
    /// the struct does not know (it would be dropped and then differ from the example).
    fn round_trip<T: Serialize + DeserializeOwned>(key: &str) {
        let example = examples()
            .remove(key)
            .unwrap_or_else(|| panic!("no example named {key:?}"));
        let value: T = serde_json::from_value(example.clone())
            .unwrap_or_else(|e| panic!("example {key:?} does not deserialize: {e}"));
        let again = serde_json::to_value(&value).expect("serializes");
        assert_eq!(again, example, "example {key:?} does not round-trip");
    }

    #[test]
    fn examples_round_trip() {
        round_trip::<AppError>("tauri.AppError");
        round_trip::<HardwareInfo>("tauri.HardwareInfo");
        round_trip::<Recommendation>("tauri.Recommendation");
        round_trip::<ModelInfo>("tauri.ModelInfo");
        round_trip::<SetupStatus>("tauri.SetupStatus");
        round_trip::<Settings>("tauri.Settings");
        round_trip::<SettingsPatch>("tauri.SettingsPatch");
        round_trip::<ConvertOptions>("tauri.ConvertOptions");
        round_trip::<QueueItem>("tauri.QueueItem");
        round_trip::<PageStatus>("tauri.PageStatus");
        round_trip::<ReviewPage>("tauri.ReviewPage");
        round_trip::<OutputFile>("tauri.OutputFile");
        round_trip::<OutputStatus>("tauri.OutputStatus");
        round_trip::<QueueProgress>("tauri.event.queue-progress");
        round_trip::<SetupStatus>("tauri.event.setup-progress");
    }

    #[test]
    fn event_names_are_the_documented_ones() {
        assert_eq!(EVENT_QUEUE_PROGRESS, "queue://progress");
        assert_eq!(EVENT_SETUP_PROGRESS, "setup://progress");
    }

    #[test]
    fn unit_enums_use_camel_case_values() {
        fn json<T: Serialize>(value: T) -> String {
            serde_json::to_string(&value).unwrap()
        }
        assert_eq!(json(QueueStatus::NeedsAttention), "\"needsAttention\"");
        assert_eq!(
            json(AppErrorCode::LlamaServerUnreachable),
            "\"llamaServerUnreachable\""
        );
        assert_eq!(json(FitBadge::WontFit), "\"wontFit\"");
        assert_eq!(json(TextMode::ExistingText), "\"existingText\"");
        assert_eq!(json(Os::Macos), "\"macos\"");
    }

    #[test]
    fn save_location_is_tagged_by_kind() {
        let folder = SaveLocation::Folder {
            path: "D:/out".into(),
        };
        assert_eq!(
            serde_json::to_value(&folder).unwrap(),
            serde_json::json!({ "kind": "folder", "path": "D:/out" })
        );
        let original: SaveLocation = serde_json::from_str(r#"{"kind":"original"}"#).unwrap();
        assert_eq!(original, SaveLocation::Original);
    }

    #[test]
    fn settings_patch_ignores_missing_and_null_fields() {
        let patch: SettingsPatch =
            serde_json::from_str(r#"{"theme":"dark","defaultModelId":null}"#).unwrap();
        assert_eq!(patch.theme, Some(Theme::Dark));
        assert_eq!(patch.default_model_id, None);
        assert_eq!(patch.models_dir, None);
    }
}
