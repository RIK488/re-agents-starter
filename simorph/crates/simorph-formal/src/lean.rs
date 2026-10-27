//! Lean 4 backend.  Lean is not bundled with SIMORPH: when `lean` is not on
//! `PATH`, every call returns `NotImplemented` with the exact reason.

use simorph_core::{Result, SimorphError};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct LeanResult {
    pub accepted: bool,
    pub output: String,
    pub lean_version: String,
    pub source_sha256: String,
    pub elapsed_ms: f64,
}

/// Locate an executable on `PATH`.
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

pub struct LeanBackend {
    pub lean: PathBuf,
    pub timeout: Duration,
}

impl LeanBackend {
    /// Detect `lean` (and report `lake`) on `PATH`.
    pub fn detect() -> Result<Self> {
        match which("lean") {
            Some(lean) => Ok(Self { lean, timeout: Duration::from_secs(120) }),
            None => Err(SimorphError::NotImplemented(format!(
                "Lean 4 backend unavailable: `lean` not found on PATH ({}); `lake` {}; install the toolchain with elan to enable machine-checked Lean proofs",
                std::env::var("PATH").unwrap_or_default(),
                if which("lake").is_some() { "found" } else { "not found" }
            ))),
        }
    }

    /// Check a Lean source file.  `accepted` is true iff Lean exits with
    /// status 0 and reports no `sorry`.
    pub fn check(&self, source: &str) -> Result<LeanResult> {
        let dir = std::env::temp_dir().join(format!("simorph-lean-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("Proof.lean");
        std::fs::write(&file, source)?;
        let start = Instant::now();
        let version = Command::new(&self.lean)
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let mut child = Command::new(&self.lean)
            .arg(&file)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if start.elapsed() > self.timeout {
                let _ = child.kill();
                return Err(SimorphError::Timeout(self.timeout.as_millis() as u64));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(LeanResult {
            accepted: out.status.success() && !text.contains("sorry"),
            output: text,
            lean_version: version,
            source_sha256: simorph_core::hashing::sha256_hex(source.as_bytes()),
            elapsed_ms: start.elapsed().as_secs_f64() * 1e3,
        })
    }
}
