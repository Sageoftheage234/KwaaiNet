//! Hardware calibration — estimate optimal block count from available RAM/VRAM

use sysinfo::System;
use tracing::debug;

/// Known model block counts (total blocks in the full model)
fn model_total_blocks(model: &str) -> u32 {
    let model = model.to_lowercase();
    if model.contains("llama-3") && model.contains("8b") {
        32
    } else if model.contains("llama-3") && model.contains("70b") {
        80
    } else if model.contains("llama-2") && model.contains("7b") {
        32
    } else if model.contains("llama-2") && model.contains("13b") {
        40
    } else if model.contains("llama-2") && model.contains("70b") {
        80
    } else {
        32 // safe default (includes mistral-7b and unknown models)
    }
}

/// Memory per block in bytes (float16)
fn bytes_per_block_f16(model: &str) -> u64 {
    let model = model.to_lowercase();
    if model.contains("70b") {
        500 * 1024 * 1024
    }
    // ~500 MB
    else if model.contains("13b") {
        312 * 1024 * 1024
    }
    // ~312 MB
    else {
        250 * 1024 * 1024
    } // ~250 MB (7-8B)
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub total_vram: u64,
    pub free_vram: u64,
}

#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub total_memory: u64,
    pub available_memory: u64,
    pub cpu_cores: usize,
    pub gpu: Option<GpuInfo>,
}

#[derive(Debug, Clone)]
pub struct CalibrationProfile {
    pub min_blocks: u32,
    pub recommended_blocks: u32,
    pub max_blocks: u32,
    pub total_blocks: u32,
    /// Whether the recommendation is based on GPU VRAM (true) or system RAM (false).
    pub gpu_based: bool,
}

impl CalibrationProfile {
    pub fn get_blocks(&self, profile: &str) -> Option<u32> {
        match profile {
            "min" => Some(self.min_blocks),
            "recommended" => Some(self.recommended_blocks),
            "max" => Some(self.max_blocks),
            _ => None,
        }
    }
}

/// Detect NVIDIA GPU via nvidia-smi.
/// Returns name, total VRAM (bytes), free VRAM (bytes).
fn detect_nvidia_gpu() -> Option<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    let parts: Vec<&str> = line.splitn(3, ',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
        return None;
    }
    let name = parts[0].to_string();
    // nvidia-smi reports MiB
    let total_mib: u64 = parts[1].parse().ok()?;
    let free_mib: u64 = parts[2].parse().ok()?;
    Some(GpuInfo {
        name,
        total_vram: total_mib * 1024 * 1024,
        free_vram: free_mib * 1024 * 1024,
    })
}

/// On macOS with Apple Silicon, GPU shares unified memory with the CPU.
/// Report the system RAM as GPU VRAM since Metal/MLX use the same pool.
#[cfg(target_os = "macos")]
fn detect_apple_gpu(sys: &System) -> Option<GpuInfo> {
    // Check for Apple Silicon via sysctl
    let output = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()?;
    let brand = String::from_utf8_lossy(&output.stdout);
    if !brand.contains("Apple") {
        return None;
    }
    // Unified memory — GPU and CPU share the same pool
    let total = sys.total_memory();
    let available = sys
        .available_memory()
        .max(total.saturating_sub(sys.used_memory()));
    Some(GpuInfo {
        name: brand.trim().to_string(),
        total_vram: total,
        free_vram: available,
    })
}

#[cfg(not(target_os = "macos"))]
fn detect_apple_gpu(_sys: &System) -> Option<GpuInfo> {
    None
}

pub struct CalibrationEngine {
    pub hardware: HardwareInfo,
}

impl CalibrationEngine {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let total = sys.total_memory();
        // available_memory() returns 0 on macOS in sysinfo 0.30; derive from used instead
        let available = sys
            .available_memory()
            .max(total.saturating_sub(sys.used_memory()));

        // GPU detection: NVIDIA first, then Apple Silicon unified memory
        let gpu = detect_nvidia_gpu().or_else(|| detect_apple_gpu(&sys));
        debug!(?gpu, "GPU detection result");

        let hardware = HardwareInfo {
            total_memory: total,
            available_memory: available,
            cpu_cores: sys.cpus().len(),
            gpu,
        };
        debug!(?hardware, "Hardware detected");
        Self { hardware }
    }

    pub fn calibrate(&self, model: &str) -> CalibrationProfile {
        let total_blocks = model_total_blocks(model);
        let bytes_per_block = bytes_per_block_f16(model);

        // If GPU is available, use VRAM for block estimation.
        // Reserve 512 MB for GPU overhead (driver, KV-cache scratch, etc.)
        // For CPU-only, reserve 2 GB for OS + other processes.
        let (usable, gpu_based) = if let Some(ref gpu) = self.hardware.gpu {
            let reserve = 512 * 1024 * 1024;
            (gpu.free_vram.saturating_sub(reserve), true)
        } else {
            let reserve = 2 * 1024 * 1024 * 1024;
            (self.hardware.available_memory.saturating_sub(reserve), false)
        };

        let max_blocks = ((usable as f64 / bytes_per_block as f64) as u32)
            .min(total_blocks)
            .max(1);

        // Recommended = 75% of max; min = 1 block or 25% of max
        let recommended_blocks = ((max_blocks as f64 * 0.75) as u32).max(1);
        let min_blocks = ((max_blocks as f64 * 0.25) as u32).max(1);

        CalibrationProfile {
            min_blocks,
            recommended_blocks,
            max_blocks,
            total_blocks,
            gpu_based,
        }
    }
}
