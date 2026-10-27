//! Detection of the resources a worker really has.

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub name: String,
    pub memory_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceInventory {
    pub hostname_hash: String,
    pub cpu_cores: usize,
    pub memory_mb: u64,
    pub gpus: Vec<GpuInfo>,
    /// How each figure was obtained (auditable, never guessed).
    pub detected_by: Vec<String>,
}

impl ResourceInventory {
    /// Probe the local machine.
    pub fn detect() -> Self {
        let mut detected_by = vec![];
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        detected_by.push("cpu: std::thread::available_parallelism".into());
        let memory_mb = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|kb| kb.parse::<u64>().ok())
            })
            .map(|kb| kb / 1024)
            .unwrap_or(0);
        detected_by.push(if memory_mb > 0 {
            "memory: /proc/meminfo MemTotal".into()
        } else {
            "memory: unknown (no /proc/meminfo)".into()
        });
        let mut gpus = nvidia_gpus();
        if gpus.is_empty() {
            gpus = rocm_gpus();
        }
        detected_by.push(if gpus.is_empty() {
            "gpu: none detected (nvidia-smi / rocm-smi absent or reported no device)".into()
        } else {
            format!("gpu: {} device(s) via {}", gpus.len(), gpus[0].vendor)
        });
        let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
        Self {
            hostname_hash: simorph_core::hashing::sha256_hex(host.trim().as_bytes())[..12]
                .to_string(),
            cpu_cores,
            memory_mb,
            gpus,
            detected_by,
        }
    }

    pub fn gpu_count(&self) -> u32 {
        self.gpus.len() as u32
    }
}

fn nvidia_gpus() -> Vec<GpuInfo> {
    let Ok(out) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split(',').map(|s| s.trim());
            let name = it.next()?.to_string();
            let mem = it.next()?.parse::<u64>().ok()?;
            Some(GpuInfo {
                vendor: "nvidia".into(),
                name,
                memory_mb: mem,
            })
        })
        .collect()
}

fn rocm_gpus() -> Vec<GpuInfo> {
    let Ok(out) = Command::new("rocm-smi")
        .args(["--showproductname", "--csv"])
        .output()
    else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter(|l| l.starts_with("card"))
        .map(|l| GpuInfo {
            vendor: "amd".into(),
            name: l.split(',').nth(1).unwrap_or("unknown").trim().to_string(),
            memory_mb: 0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_consistent_with_tools() {
        let inv = ResourceInventory::detect();
        assert!(inv.cpu_cores >= 1);
        let has_nvidia = Command::new("nvidia-smi")
            .arg("-L")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !has_nvidia {
            assert!(
                inv.gpus.iter().all(|g| g.vendor != "nvidia"),
                "GPU reported without nvidia-smi"
            );
        }
        assert!(inv.detected_by.iter().any(|d| d.starts_with("gpu:")));
    }
}
