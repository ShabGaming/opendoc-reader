//! Hardware suggester (M1-6): evaluates the rules in `manifest.suggester` against a probe.
//!
//! The rules are data (docs/adr/0008). This module only matches conditions, fills the reason
//! text and attaches a speed estimate to each model.

use std::collections::HashMap;

use crate::contract::{
    Backend, FitBadge, ModelFit, Os, Recommendation, SpeedBasis, SpeedEstimate, Tier,
};
use crate::hardware::{primary_gpu, RawProbe};
use crate::manifest::{Arch, Manifest, ModelEntry, Range, When};

/// Rough size of the engine's Python environment (the spike measured about 0.9 GB).
const ENGINE_ENV_ESTIMATE_BYTES: u64 = 900_000_000;

/// The probe quantities the rules talk about. 0 means "does not apply" (docs: manifest schema,
/// `when`): `vram_mb` is 0 on the CPU and Metal backends and `unified_mb` is 0 off Apple Silicon.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggesterInput {
    pub os: Os,
    pub backend: Backend,
    pub vram_mb: u64,
    pub ram_mb: u64,
    pub unified_mb: u64,
    pub gpu_name: String,
}

pub fn input_from(probe: &RawProbe, backend: Backend) -> SuggesterInput {
    let gpu = primary_gpu(probe, backend);
    let on_gpu = matches!(backend, Backend::Cuda | Backend::Vulkan);
    SuggesterInput {
        os: probe.os,
        backend,
        vram_mb: if on_gpu {
            gpu.and_then(|g| g.vram_mb).unwrap_or(0)
        } else {
            0
        },
        ram_mb: probe.ram_mb,
        unified_mb: probe.unified_memory_mb.unwrap_or(0),
        gpu_name: gpu.map(|g| g.name.clone()).unwrap_or_default(),
    }
}

fn in_range(range: &Option<Range>, value: u64) -> bool {
    range.as_ref().is_none_or(|r| r.contains(value))
}

pub fn matches(when: &When, input: &SuggesterInput) -> bool {
    when.backend
        .as_ref()
        .is_none_or(|backends| backends.contains(&input.backend))
        && in_range(&when.vram_mb, input.vram_mb)
        && in_range(&when.ram_mb, input.ram_mb)
        && in_range(&when.unified_memory_mb, input.unified_mb)
}

fn gigabytes(mb: u64) -> u64 {
    ((mb as f64) / 1024.0).round() as u64
}

/// Fills `{gpuName}`, `{vramGB}`, `{ramGB}` and `{unifiedGB}` in a rule's reason.
pub fn fill_reason(template: &str, input: &SuggesterInput) -> String {
    template
        .replace("{gpuName}", &input.gpu_name)
        .replace("{vramGB}", &gigabytes(input.vram_mb).to_string())
        .replace("{ramGB}", &gigabytes(input.ram_mb).to_string())
        .replace("{unifiedGB}", &gigabytes(input.unified_mb).to_string())
}

/// The recommended model and its reason: the first matching rule, else the default model.
pub fn recommend(manifest: &Manifest, input: &SuggesterInput) -> (String, String) {
    let suggester = &manifest.suggester;
    if let Some(rule) = suggester
        .recommend
        .iter()
        .find(|rule| matches(&rule.when, input))
    {
        return (rule.model.clone(), fill_reason(&rule.reason, input));
    }
    (
        suggester.default_model.clone(),
        "This is the best all-round choice for most computers.".to_string(),
    )
}

/// The fit badge of a model: the first rule that names it and matches, else `wontFit`.
pub fn badge_for(manifest: &Manifest, model_id: &str, input: &SuggesterInput) -> FitBadge {
    manifest
        .suggester
        .badges
        .iter()
        .find(|rule| rule.models.iter().any(|m| m == model_id) && matches(&rule.when, input))
        .map(|rule| rule.badge)
        .unwrap_or(FitBadge::WontFit)
}

/// Seconds per page before this computer has been timed.
///
/// The GPU figures are medians from the model spike (docs/spike), measured on one RTX 4090, so
/// they are the best case; Apple Silicon and CPU figures are estimates derived from the spike's
/// CPU runs and the memory-bandwidth argument in its Apple Silicon section.
fn table_speed(model: &ModelEntry, input: &SuggesterInput) -> SpeedEstimate {
    let pro_mac = input.unified_mb >= 18 * 1024;
    let (seconds, basis) = match (model.tier, model.experimental, input.backend) {
        (Tier::Fast, _, _) => (8.4, SpeedBasis::Estimated),
        (Tier::Accurate, _, Backend::Cuda) => (6.7, SpeedBasis::Measured),
        (Tier::Accurate, _, Backend::Vulkan) => (10.0, SpeedBasis::Estimated),
        (Tier::Accurate, _, Backend::Metal) => {
            (if pro_mac { 10.0 } else { 25.0 }, SpeedBasis::Estimated)
        }
        (Tier::Accurate, _, Backend::Cpu) => (60.0, SpeedBasis::Estimated),
        (Tier::Max, true, Backend::Cuda | Backend::Vulkan) => (3.0, SpeedBasis::Estimated),
        (Tier::Max, true, Backend::Metal) => {
            (if pro_mac { 10.0 } else { 18.0 }, SpeedBasis::Estimated)
        }
        (Tier::Max, true, Backend::Cpu) => (35.0, SpeedBasis::Estimated),
        (Tier::Max, false, Backend::Cuda) => (2.7, SpeedBasis::Measured),
        (Tier::Max, false, Backend::Vulkan) => (5.0, SpeedBasis::Estimated),
        (Tier::Max, false, Backend::Metal) => {
            (if pro_mac { 10.0 } else { 15.0 }, SpeedBasis::Estimated)
        }
        (Tier::Max, false, Backend::Cpu) => (28.0, SpeedBasis::Estimated),
    };
    SpeedEstimate {
        seconds_per_page: seconds,
        basis,
    }
}

/// Builds the recommendation. `calibrated` holds seconds per page measured on this computer by
/// `calibrate.run`, by model id; a measured value replaces the table.
pub fn build_recommendation(
    manifest: &Manifest,
    input: &SuggesterInput,
    calibrated: &HashMap<String, f64>,
) -> Recommendation {
    let (model_id, reason) = recommend(manifest, input);
    let fits = manifest
        .models
        .iter()
        .map(|(id, model)| {
            let badge = badge_for(manifest, id, input);
            let speed = match badge {
                FitBadge::Hidden | FitBadge::WontFit => None,
                _ => Some(match calibrated.get(id) {
                    Some(seconds) => SpeedEstimate {
                        seconds_per_page: *seconds,
                        basis: SpeedBasis::Calibrated,
                    },
                    None => table_speed(model, input),
                }),
            };
            ModelFit {
                model_id: id.clone(),
                badge,
                speed,
            }
        })
        .collect();

    let needs_llama = manifest
        .model(&model_id)
        .is_some_and(ModelEntry::needs_llama_server);
    let llama_bytes: u64 = if needs_llama {
        let arch = match input.os {
            Os::Windows => Arch::X64,
            Os::Macos => Arch::Arm64,
        };
        manifest
            .llama_build(input.os, arch, input.backend)
            .map(|(_, build)| build.files.iter().map(|f| f.size_bytes).sum())
            .unwrap_or(0)
    } else {
        0
    };
    Recommendation {
        model_id,
        reason,
        fits,
        runtime_download_bytes: llama_bytes + ENGINE_ENV_ESTIMATE_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{GpuInfo, GpuVendor};
    use crate::hardware::{select_backend, RawProbe};

    struct Profile {
        name: &'static str,
        probe: RawProbe,
        vulkan: bool,
        recommended: &'static str,
        reason_contains: &'static str,
        /// fast, accurate, max, experimental
        badges: [FitBadge; 4],
    }

    fn windows_pc(gpus: Vec<GpuInfo>, nvidia: bool, ram_mb: u64) -> RawProbe {
        RawProbe {
            os: Os::Windows,
            gpus,
            nvidia_driver: nvidia.then(|| "576.80".to_string()),
            cuda_driver_api: nvidia.then_some((13, 4)),
            ram_mb,
            unified_memory_mb: None,
            cpu_name: "Test CPU".into(),
            cpu_cores: 8,
        }
    }

    fn mac(unified_mb: u64) -> RawProbe {
        RawProbe {
            os: Os::Macos,
            gpus: vec![GpuInfo {
                vendor: GpuVendor::Apple,
                name: "Apple M-series".into(),
                vram_mb: None,
            }],
            nvidia_driver: None,
            cuda_driver_api: None,
            ram_mb: unified_mb,
            unified_memory_mb: Some(unified_mb),
            cpu_name: "Apple M-series".into(),
            cpu_cores: 8,
        }
    }

    fn gpu(vendor: GpuVendor, name: &str, vram_mb: u64) -> GpuInfo {
        GpuInfo {
            vendor,
            name: name.into(),
            vram_mb: Some(vram_mb),
        }
    }

    use FitBadge::{Hidden, RunsWell, Slow, WontFit};

    fn profiles() -> Vec<Profile> {
        vec![
            Profile {
                name: "RTX 3060 12 GB, 32 GB RAM",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Nvidia, "NVIDIA GeForce RTX 3060", 12288)],
                    true,
                    32768,
                ),
                vulkan: false,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "Your NVIDIA GeForce RTX 3060 (12 GB) runs Max on the GPU.",
                badges: [RunsWell, RunsWell, RunsWell, RunsWell],
            },
            Profile {
                name: "RTX 3050 4 GB, 16 GB RAM",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Nvidia, "NVIDIA GeForce RTX 3050", 4096)],
                    true,
                    16384,
                ),
                vulkan: false,
                recommended: "accurate-paddleocr-vl-1.6",
                reason_contains: "best all-round choice",
                badges: [RunsWell, RunsWell, Slow, Hidden],
            },
            Profile {
                name: "RTX 4090 24 GB",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Nvidia, "NVIDIA GeForce RTX 4090", 24564)],
                    true,
                    65536,
                ),
                vulkan: false,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "(24 GB)",
                badges: [RunsWell, RunsWell, RunsWell, RunsWell],
            },
            Profile {
                name: "AMD RX 7800 XT, default settings, 32 GB RAM",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Amd, "AMD Radeon RX 7800 XT", 16384)],
                    false,
                    32768,
                ),
                vulkan: false,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "with 32 GB of RAM Max measured faster",
                badges: [RunsWell, Slow, Slow, Hidden],
            },
            Profile {
                name: "AMD RX 7800 XT with experimental Vulkan",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Amd, "AMD Radeon RX 7800 XT", 16384)],
                    false,
                    32768,
                ),
                vulkan: true,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "Your AMD Radeon RX 7800 XT (16 GB) runs Max on the GPU.",
                badges: [RunsWell, RunsWell, RunsWell, RunsWell],
            },
            Profile {
                name: "Intel laptop, integrated graphics, 8 GB RAM",
                probe: windows_pc(
                    vec![gpu(GpuVendor::Intel, "Intel Iris Xe", 128)],
                    false,
                    7900,
                ),
                vulkan: false,
                recommended: "fast-ppocrv6-tiny",
                reason_contains: "8 GB of RAM",
                badges: [RunsWell, Slow, WontFit, Hidden],
            },
            Profile {
                name: "CPU only, exactly 16 GB RAM",
                probe: windows_pc(vec![], false, 16384),
                vulkan: false,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "with 16 GB of RAM",
                badges: [RunsWell, Slow, Slow, Hidden],
            },
            Profile {
                name: "CPU only, 12 GB RAM (no rule matches)",
                probe: windows_pc(vec![], false, 12288),
                vulkan: false,
                recommended: "accurate-paddleocr-vl-1.6",
                reason_contains: "best all-round choice",
                badges: [RunsWell, Slow, Slow, Hidden],
            },
            Profile {
                name: "Apple Silicon, 16 GB",
                probe: mac(16384),
                vulkan: false,
                recommended: "max-deepseek-ocr-1-q8",
                reason_contains: "Your Mac has 16 GB of memory, enough to run Max.",
                badges: [RunsWell, RunsWell, RunsWell, RunsWell],
            },
            Profile {
                name: "Apple Silicon, 8 GB",
                probe: mac(8192),
                vulkan: false,
                recommended: "accurate-paddleocr-vl-1.6",
                reason_contains: "best all-round choice",
                badges: [RunsWell, Slow, WontFit, Hidden],
            },
        ]
    }

    #[test]
    fn profiles_get_the_expected_recommendation_and_badges() {
        let manifest = Manifest::embedded();
        let order = [
            "fast-ppocrv6-tiny",
            "accurate-paddleocr-vl-1.6",
            "max-deepseek-ocr-1-q8",
            "experimental-deepseek-ocr-2-q8",
        ];
        for profile in profiles() {
            let backend = select_backend(&profile.probe, Some((13, 4)), profile.vulkan);
            let input = input_from(&profile.probe, backend);
            let rec = build_recommendation(manifest, &input, &HashMap::new());
            assert_eq!(rec.model_id, profile.recommended, "{}", profile.name);
            assert!(
                rec.reason.contains(profile.reason_contains),
                "{}: {:?} does not contain {:?}",
                profile.name,
                rec.reason,
                profile.reason_contains
            );
            let badges: Vec<FitBadge> = order
                .iter()
                .map(|id| rec.fits.iter().find(|f| f.model_id == *id).unwrap().badge)
                .collect();
            assert_eq!(badges, profile.badges.to_vec(), "{}", profile.name);
        }
    }

    #[test]
    fn hidden_and_wont_fit_models_have_no_speed() {
        let manifest = Manifest::embedded();
        let probe = mac(8192);
        let input = input_from(&probe, Backend::Metal);
        let rec = build_recommendation(manifest, &input, &HashMap::new());
        for fit in &rec.fits {
            match fit.badge {
                Hidden | WontFit => assert!(fit.speed.is_none(), "{}", fit.model_id),
                _ => assert!(fit.speed.is_some(), "{}", fit.model_id),
            }
        }
    }

    #[test]
    fn calibration_replaces_the_table_estimate() {
        let manifest = Manifest::embedded();
        let probe = windows_pc(vec![], false, 16384);
        let input = input_from(&probe, Backend::Cpu);
        let mut calibrated = HashMap::new();
        calibrated.insert("accurate-paddleocr-vl-1.6".to_string(), 31.5);
        let rec = build_recommendation(manifest, &input, &calibrated);
        let accurate = rec
            .fits
            .iter()
            .find(|f| f.model_id == "accurate-paddleocr-vl-1.6")
            .unwrap();
        let speed = accurate.speed.as_ref().unwrap();
        assert_eq!(speed.basis, SpeedBasis::Calibrated);
        assert_eq!(speed.seconds_per_page, 31.5);
        let fast = rec
            .fits
            .iter()
            .find(|f| f.model_id == "fast-ppocrv6-tiny")
            .unwrap();
        assert_eq!(fast.speed.as_ref().unwrap().basis, SpeedBasis::Estimated);
    }

    #[test]
    fn the_runtime_download_includes_llama_server_only_when_needed() {
        let manifest = Manifest::embedded();
        let nvidia = windows_pc(
            vec![gpu(GpuVendor::Nvidia, "NVIDIA GeForce RTX 3060", 12288)],
            true,
            32768,
        );
        let input = input_from(&nvidia, Backend::Cuda);
        let max = build_recommendation(manifest, &input, &HashMap::new());
        assert_eq!(
            max.runtime_download_bytes,
            154_582_227 + 423_535_356 + 900_000_000
        );

        let small = windows_pc(vec![], false, 4096);
        let input = input_from(&small, Backend::Cpu);
        let fast = build_recommendation(manifest, &input, &HashMap::new());
        assert_eq!(fast.model_id, "fast-ppocrv6-tiny");
        assert_eq!(fast.runtime_download_bytes, 900_000_000);
    }

    #[test]
    fn gigabytes_round_to_the_nearest_whole_number() {
        assert_eq!(gigabytes(12288), 12);
        assert_eq!(gigabytes(24564), 24);
        assert_eq!(gigabytes(7900), 8);
    }

    #[test]
    fn vram_does_not_apply_on_cpu_or_metal() {
        let probe = windows_pc(vec![gpu(GpuVendor::Amd, "RX", 16384)], false, 16384);
        let cpu = input_from(&probe, Backend::Cpu);
        assert_eq!(cpu.vram_mb, 0);
        let mac_input = input_from(&mac(16384), Backend::Metal);
        assert_eq!(mac_input.vram_mb, 0);
        assert_eq!(mac_input.unified_mb, 16384);
    }
}
