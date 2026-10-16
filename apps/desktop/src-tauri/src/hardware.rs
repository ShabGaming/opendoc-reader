//! Hardware probe (M1-5): GPUs, memory, CPU and free disk.
//!
//! Windows: NVIDIA GPUs come from NVML (`nvml-wrapper`), which also reports the driver and the
//! CUDA version it supports; when there is no NVIDIA driver NVML is simply unavailable. AMD and
//! Intel adapters (and any NVIDIA adapter NVML did not list) come from DXGI. macOS: chip and
//! memory from `sysctl`.

use std::path::{Path, PathBuf};

use crate::contract::{Backend, GpuInfo, GpuVendor, HardwareInfo, Os};
use crate::paths::path_str;

/// What the probe found, before a backend is chosen. The backend depends on a setting
/// (`experimentalVulkan`), so it is derived from this on demand.
#[derive(Debug, Clone, PartialEq)]
pub struct RawProbe {
    pub os: Os,
    pub gpus: Vec<GpuInfo>,
    /// True when NVML answered, i.e. an NVIDIA driver is installed and working.
    pub nvidia_driver: Option<String>,
    /// Highest CUDA version the installed driver supports, as `(major, minor)`.
    pub cuda_driver_api: Option<(u32, u32)>,
    pub ram_mb: u64,
    pub unified_memory_mb: Option<u64>,
    pub cpu_name: String,
    pub cpu_cores: u32,
}

pub fn classify_vendor(vendor_id: u32) -> GpuVendor {
    match vendor_id {
        0x10DE => GpuVendor::Nvidia,
        0x1002 | 0x1022 => GpuVendor::Amd,
        0x8086 => GpuVendor::Intel,
        0x106B => GpuVendor::Apple,
        _ => GpuVendor::Other,
    }
}

/// `(major, minor)` from NVML's integer form: 13040 is CUDA 13.4.
pub fn cuda_version_from_nvml(value: i32) -> Option<(u32, u32)> {
    if value <= 0 {
        return None;
    }
    let value = value as u32;
    Some((value / 1000, (value % 1000) / 10))
}

/// Picks the llama-server backend for a probe.
///
/// NVIDIA with a working driver gets CUDA when the driver supports the pinned build's CUDA
/// version. AMD, Intel and NVIDIA without CUDA get Vulkan only when the experimental setting is
/// on, and the CPU build otherwise. Apple Silicon gets Metal.
pub fn select_backend(
    probe: &RawProbe,
    required_cuda: Option<(u32, u32)>,
    vulkan_enabled: bool,
) -> Backend {
    if probe.os == Os::Macos {
        return if probe.unified_memory_mb.is_some() {
            Backend::Metal
        } else {
            Backend::Cpu
        };
    }
    let has_nvidia = probe.gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia);
    if has_nvidia && probe.nvidia_driver.is_some() {
        let supported = match (probe.cuda_driver_api, required_cuda) {
            (Some(have), Some(need)) => have >= need,
            (None, Some(_)) => false,
            (_, None) => true,
        };
        if supported {
            return Backend::Cuda;
        }
    }
    let has_gpu = probe.gpus.iter().any(|g| {
        matches!(
            g.vendor,
            GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel
        )
    });
    if vulkan_enabled && has_gpu {
        return Backend::Vulkan;
    }
    Backend::Cpu
}

/// The GPU llama-server would run on for this backend: the one with the most video memory
/// among the vendors the backend serves.
pub fn primary_gpu(probe: &RawProbe, backend: Backend) -> Option<&GpuInfo> {
    let wanted = |gpu: &&GpuInfo| match backend {
        Backend::Cuda => gpu.vendor == GpuVendor::Nvidia,
        Backend::Vulkan => matches!(
            gpu.vendor,
            GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel
        ),
        Backend::Metal => gpu.vendor == GpuVendor::Apple,
        Backend::Cpu => false,
    };
    probe
        .gpus
        .iter()
        .filter(wanted)
        .max_by_key(|gpu| gpu.vram_mb.unwrap_or(0))
}

pub fn to_info(
    probe: &RawProbe,
    backend: Backend,
    models_dir: &Path,
    free_disk_mb: u64,
) -> HardwareInfo {
    HardwareInfo {
        os: probe.os,
        gpus: probe.gpus.clone(),
        backend,
        ram_mb: probe.ram_mb,
        unified_memory_mb: probe.unified_memory_mb,
        cpu_name: probe.cpu_name.clone(),
        cpu_cores: probe.cpu_cores,
        models_dir: path_str(models_dir),
        free_disk_mb,
        cuda_driver_version: probe.nvidia_driver.clone(),
    }
}

pub fn current_os() -> Os {
    if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Windows
    }
}

/// Probes the machine. Takes a few hundred milliseconds, so call it from a blocking task.
pub fn probe_raw() -> RawProbe {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.refresh_cpu_all();
    let ram_mb = system.total_memory() / (1024 * 1024);
    let cpu_name = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown CPU".to_string());
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or_else(|_| system.cpus().len().max(1) as u32);

    let mut probe = RawProbe {
        os: current_os(),
        gpus: Vec::new(),
        nvidia_driver: None,
        cuda_driver_api: None,
        ram_mb,
        unified_memory_mb: None,
        cpu_name,
        cpu_cores,
    };
    platform::fill(&mut probe);
    probe
}

/// Free space, in MB, on the drive that holds `path` (or the nearest folder that exists).
pub fn free_disk_mb(path: &Path) -> u64 {
    let mut target: PathBuf = path.to_path_buf();
    while !target.exists() {
        if !target.pop() {
            break;
        }
    }
    let Ok(canonical) = std::fs::canonicalize(&target) else {
        return 0;
    };
    let canonical = strip_verbatim_prefix(&canonical);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| canonical.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space() / (1024 * 1024))
        .unwrap_or(0)
}

fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    pub fn fill(probe: &mut RawProbe) {
        let nvml = nvidia_via_nvml();
        if let Some((gpus, driver, cuda)) = nvml {
            probe.gpus.extend(gpus);
            probe.nvidia_driver = Some(driver);
            probe.cuda_driver_api = cuda;
        }
        let nvml_listed_nvidia = probe.nvidia_driver.is_some();
        for gpu in dxgi_adapters() {
            if gpu.vendor == GpuVendor::Nvidia && nvml_listed_nvidia {
                continue;
            }
            probe.gpus.push(gpu);
        }
    }

    type NvmlResult = (Vec<GpuInfo>, String, Option<(u32, u32)>);

    fn nvidia_via_nvml() -> Option<NvmlResult> {
        let nvml = match nvml_wrapper::Nvml::init() {
            Ok(nvml) => nvml,
            Err(error) => {
                tracing::info!("NVML is not available: {error}");
                return None;
            }
        };
        let driver = nvml.sys_driver_version().ok()?;
        let cuda = nvml
            .sys_cuda_driver_version()
            .ok()
            .and_then(cuda_version_from_nvml);
        let count = nvml.device_count().ok()?;
        let mut gpus = Vec::new();
        for index in 0..count {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };
            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
            let vram_mb = device
                .memory_info()
                .ok()
                .map(|memory| memory.total / (1024 * 1024));
            gpus.push(GpuInfo {
                vendor: GpuVendor::Nvidia,
                name,
                vram_mb,
            });
        }
        Some((gpus, driver, cuda))
    }

    /// Hardware adapters Windows knows about. The software "Basic Render Driver" is skipped.
    fn dxgi_adapters() -> Vec<GpuInfo> {
        const SOFTWARE_FLAG: u32 = 2;
        let mut found = Vec::new();
        // SAFETY: plain DXGI enumeration; every returned interface is owned and released on drop.
        unsafe {
            let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
                return found;
            };
            let mut index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(index) {
                index += 1;
                let Ok(desc) = adapter.GetDesc1() else {
                    continue;
                };
                if desc.Flags & SOFTWARE_FLAG != 0 {
                    continue;
                }
                let name = clean_adapter_name(&desc.Description);
                if name.starts_with("Microsoft Basic") {
                    continue;
                }
                found.push(GpuInfo {
                    vendor: classify_vendor(desc.VendorId),
                    name,
                    vram_mb: Some(desc.DedicatedVideoMemory as u64 / (1024 * 1024)),
                });
            }
        }
        found
    }

    fn clean_adapter_name(raw: &[u16]) -> String {
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        String::from_utf16_lossy(&raw[..end]).trim().to_string()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn fill(probe: &mut RawProbe) {
        if let Some(name) = sysctl("machdep.cpu.brand_string") {
            probe.cpu_name = name;
        }
        if let Some(bytes) = sysctl("hw.memsize").and_then(|v| parse_memsize(&v)) {
            probe.ram_mb = bytes / (1024 * 1024);
        }
        let arm = sysctl("hw.optional.arm64").is_some_and(|v| v.trim() == "1");
        if arm {
            probe.unified_memory_mb = Some(probe.ram_mb);
            probe.gpus.push(GpuInfo {
                vendor: GpuVendor::Apple,
                name: probe.cpu_name.clone(),
                vram_mb: None,
            });
        }
    }

    fn sysctl(name: &str) -> Option<String> {
        let output = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", name])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::*;

    pub fn fill(_probe: &mut RawProbe) {}
}

/// Parses the output of `sysctl -n hw.memsize`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_memsize(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: GpuVendor, name: &str, vram_mb: Option<u64>) -> GpuInfo {
        GpuInfo {
            vendor,
            name: name.into(),
            vram_mb,
        }
    }

    fn windows(gpus: Vec<GpuInfo>, driver: Option<&str>, cuda: Option<(u32, u32)>) -> RawProbe {
        RawProbe {
            os: Os::Windows,
            gpus,
            nvidia_driver: driver.map(String::from),
            cuda_driver_api: cuda,
            ram_mb: 16384,
            unified_memory_mb: None,
            cpu_name: "Test CPU".into(),
            cpu_cores: 8,
        }
    }

    const CUDA_13_4: Option<(u32, u32)> = Some((13, 4));

    #[test]
    fn vendor_ids_are_classified() {
        assert_eq!(classify_vendor(0x10DE), GpuVendor::Nvidia);
        assert_eq!(classify_vendor(0x1002), GpuVendor::Amd);
        assert_eq!(classify_vendor(0x8086), GpuVendor::Intel);
        assert_eq!(classify_vendor(0x1414), GpuVendor::Other);
    }

    #[test]
    fn nvml_cuda_versions_are_split() {
        assert_eq!(cuda_version_from_nvml(13040), Some((13, 4)));
        assert_eq!(cuda_version_from_nvml(12020), Some((12, 2)));
        assert_eq!(cuda_version_from_nvml(0), None);
    }

    #[test]
    fn nvidia_with_a_new_enough_driver_gets_cuda() {
        let probe = windows(
            vec![gpu(GpuVendor::Nvidia, "RTX 4090", Some(24564))],
            Some("616.56"),
            Some((13, 4)),
        );
        assert_eq!(select_backend(&probe, CUDA_13_4, false), Backend::Cuda);
    }

    #[test]
    fn nvidia_with_an_old_driver_falls_back_to_cpu_by_default() {
        let probe = windows(
            vec![gpu(GpuVendor::Nvidia, "GTX 1060", Some(6144))],
            Some("470.00"),
            Some((11, 4)),
        );
        assert_eq!(select_backend(&probe, CUDA_13_4, false), Backend::Cpu);
        assert_eq!(select_backend(&probe, CUDA_13_4, true), Backend::Vulkan);
    }

    #[test]
    fn nvidia_without_nvml_is_not_cuda() {
        let probe = windows(
            vec![gpu(GpuVendor::Nvidia, "RTX 3060", Some(12288))],
            None,
            None,
        );
        assert_eq!(select_backend(&probe, CUDA_13_4, false), Backend::Cpu);
    }

    #[test]
    fn amd_and_intel_use_cpu_unless_vulkan_is_enabled() {
        let amd = windows(
            vec![gpu(GpuVendor::Amd, "RX 7800 XT", Some(16384))],
            None,
            None,
        );
        assert_eq!(select_backend(&amd, CUDA_13_4, false), Backend::Cpu);
        assert_eq!(select_backend(&amd, CUDA_13_4, true), Backend::Vulkan);
        let intel = windows(
            vec![gpu(GpuVendor::Intel, "Iris Xe", Some(128))],
            None,
            None,
        );
        assert_eq!(select_backend(&intel, CUDA_13_4, false), Backend::Cpu);
        assert_eq!(select_backend(&intel, CUDA_13_4, true), Backend::Vulkan);
    }

    #[test]
    fn no_gpu_is_cpu_even_with_vulkan_enabled() {
        let probe = windows(vec![], None, None);
        assert_eq!(select_backend(&probe, CUDA_13_4, true), Backend::Cpu);
    }

    #[test]
    fn apple_silicon_gets_metal() {
        let probe = RawProbe {
            os: Os::Macos,
            gpus: vec![gpu(GpuVendor::Apple, "Apple M2 Pro", None)],
            nvidia_driver: None,
            cuda_driver_api: None,
            ram_mb: 16384,
            unified_memory_mb: Some(16384),
            cpu_name: "Apple M2 Pro".into(),
            cpu_cores: 10,
        };
        assert_eq!(select_backend(&probe, None, false), Backend::Metal);
    }

    #[test]
    fn the_primary_gpu_is_the_one_with_the_most_memory_for_the_backend() {
        let probe = windows(
            vec![
                gpu(GpuVendor::Intel, "UHD", Some(128)),
                gpu(GpuVendor::Nvidia, "RTX 3060", Some(12288)),
                gpu(GpuVendor::Nvidia, "GTX 1650", Some(4096)),
            ],
            Some("576.80"),
            Some((13, 4)),
        );
        assert_eq!(
            primary_gpu(&probe, Backend::Cuda).map(|g| g.name.as_str()),
            Some("RTX 3060")
        );
        assert!(primary_gpu(&probe, Backend::Cpu).is_none());
    }

    #[test]
    fn memsize_output_is_parsed() {
        assert_eq!(parse_memsize("17179869184\n"), Some(17179869184));
        assert_eq!(parse_memsize("nope"), None);
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\x")),
            PathBuf::from(r"C:\Users\x")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new("/home/x")),
            PathBuf::from("/home/x")
        );
    }

    #[test]
    fn free_disk_of_an_existing_folder_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_disk_mb(&dir.path().join("not").join("yet")) > 0);
    }

    #[test]
    fn the_real_probe_finds_memory_and_cores() {
        let probe = probe_raw();
        assert!(probe.ram_mb > 0);
        assert!(probe.cpu_cores > 0);
        assert!(!probe.cpu_name.is_empty());
    }
}
