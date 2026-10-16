//! Settings stored as JSON with defaults (docs/design.md, section 4.5).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::contract::{ExportFormat, SaveLocation, Settings, SettingsPatch, Theme};
use crate::error::Result;
use crate::paths::path_str;

pub fn defaults(models_dir: &std::path::Path) -> Settings {
    Settings {
        default_formats: vec![ExportFormat::Docx, ExportFormat::Markdown],
        default_model_id: None,
        save_to: SaveLocation::Original,
        theme: Theme::System,
        check_for_updates: true,
        models_dir: path_str(models_dir),
        experimental_vulkan: false,
    }
}

/// What is on disk. Every field is optional so a file from an older or newer version, or one
/// edited by hand, still loads; the missing fields take their defaults.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredSettings {
    default_formats: Option<Vec<ExportFormat>>,
    default_model_id: Option<String>,
    save_to: Option<SaveLocation>,
    theme: Option<Theme>,
    check_for_updates: Option<bool>,
    models_dir: Option<String>,
    experimental_vulkan: Option<bool>,
}

fn merge(stored: StoredSettings, base: Settings) -> Settings {
    Settings {
        default_formats: stored
            .default_formats
            .filter(|formats| !formats.is_empty())
            .unwrap_or(base.default_formats),
        default_model_id: stored.default_model_id.or(base.default_model_id),
        save_to: stored.save_to.unwrap_or(base.save_to),
        theme: stored.theme.unwrap_or(base.theme),
        check_for_updates: stored.check_for_updates.unwrap_or(base.check_for_updates),
        models_dir: stored
            .models_dir
            .filter(|dir| !dir.is_empty())
            .unwrap_or(base.models_dir),
        experimental_vulkan: stored
            .experimental_vulkan
            .unwrap_or(base.experimental_vulkan),
    }
}

/// Applies a patch to settings. A missing or `null` field is left alone; an empty
/// `defaultModelId` clears the default (the app then uses the recommendation).
pub fn apply_patch(current: &Settings, patch: SettingsPatch) -> Settings {
    let mut next = current.clone();
    if let Some(formats) = patch.default_formats.filter(|formats| !formats.is_empty()) {
        next.default_formats = formats;
    }
    if let Some(model) = patch.default_model_id {
        next.default_model_id = if model.is_empty() { None } else { Some(model) };
    }
    if let Some(save_to) = patch.save_to {
        next.save_to = save_to;
    }
    if let Some(theme) = patch.theme {
        next.theme = theme;
    }
    if let Some(check) = patch.check_for_updates {
        next.check_for_updates = check;
    }
    if let Some(dir) = patch.models_dir.filter(|dir| !dir.is_empty()) {
        next.models_dir = dir;
    }
    if let Some(vulkan) = patch.experimental_vulkan {
        next.experimental_vulkan = vulkan;
    }
    next
}

pub struct SettingsStore {
    file: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads the settings file. A file that does not parse is moved aside to `settings.json.bad`
    /// and the defaults are used, so a damaged file never blocks the app from starting.
    pub fn load(file: PathBuf, default_models_dir: &std::path::Path) -> Self {
        let base = defaults(default_models_dir);
        let current = match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<StoredSettings>(&text) {
                Ok(stored) => merge(stored, base),
                Err(error) => {
                    tracing::warn!("settings file is damaged ({error}); using defaults");
                    let _ = std::fs::rename(&file, file.with_extension("json.bad"));
                    base
                }
            },
            Err(_) => base,
        };
        Self {
            file,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings lock").clone()
    }

    /// Replaces the settings and writes them to disk atomically.
    pub fn replace(&self, next: Settings) -> Result<Settings> {
        let text = serde_json::to_string_pretty(&next)?;
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = self.file.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.file)?;
        *self.current.lock().expect("settings lock") = next.clone();
        Ok(next)
    }

    pub fn patch(&self, patch: SettingsPatch) -> Result<Settings> {
        let next = apply_patch(&self.get(), patch);
        self.replace(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &std::path::Path) -> SettingsStore {
        SettingsStore::load(dir.join("settings.json"), &dir.join("models"))
    }

    #[test]
    fn defaults_match_the_design() {
        let dir = tempfile::tempdir().unwrap();
        let settings = store(dir.path()).get();
        assert_eq!(
            settings.default_formats,
            vec![ExportFormat::Docx, ExportFormat::Markdown]
        );
        assert_eq!(settings.save_to, SaveLocation::Original);
        assert_eq!(settings.theme, Theme::System);
        assert!(settings.check_for_updates);
        assert!(!settings.experimental_vulkan);
        assert_eq!(settings.default_model_id, None);
    }

    #[test]
    fn a_patch_survives_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let first = store(dir.path());
        first
            .patch(SettingsPatch {
                theme: Some(Theme::Dark),
                save_to: Some(SaveLocation::Folder {
                    path: "D:/out".into(),
                }),
                experimental_vulkan: Some(true),
                ..Default::default()
            })
            .unwrap();
        let second = store(dir.path()).get();
        assert_eq!(second.theme, Theme::Dark);
        assert!(second.experimental_vulkan);
        assert_eq!(
            second.save_to,
            SaveLocation::Folder {
                path: "D:/out".into()
            }
        );
        assert!(second.check_for_updates);
    }

    #[test]
    fn an_empty_default_model_clears_it() {
        let current = Settings {
            default_model_id: Some("fast-ppocrv6-tiny".into()),
            ..defaults(std::path::Path::new("m"))
        };
        let patch = SettingsPatch {
            default_model_id: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(apply_patch(&current, patch).default_model_id, None);
    }

    #[test]
    fn at_least_one_format_stays_selected() {
        let current = defaults(std::path::Path::new("m"));
        let patch = SettingsPatch {
            default_formats: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(apply_patch(&current, patch).default_formats.len(), 2);
    }

    #[test]
    fn a_damaged_file_falls_back_to_defaults_and_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), "{ not json").unwrap();
        let settings = store(dir.path()).get();
        assert_eq!(settings.theme, Theme::System);
        assert!(dir.path().join("settings.json.bad").exists());
    }

    #[test]
    fn unknown_and_missing_keys_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"theme":"light","somethingNew":1}"#,
        )
        .unwrap();
        let settings = store(dir.path()).get();
        assert_eq!(settings.theme, Theme::Light);
        assert!(settings.check_for_updates);
    }
}
