//! Capture of the execution environment (§11: "versions logicielles,
//! environnement d'exécution").

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Snapshot of the software environment, captured once per ledger session.
/// Deliberately contains no hostname or user name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentCapture {
    pub rustc_version: Option<String>,
    pub cargo_version: Option<String>,
    pub os: String,
    pub arch: String,
    pub crate_name: String,
    pub crate_version: String,
    /// Git commit of the SIMORPH checkout, if discoverable.
    pub git_commit: Option<String>,
    /// Additional versions recorded by the caller (e.g. plugin crates).
    #[serde(default)]
    pub extra_versions: Vec<(String, String)>,
    /// `rustc --version` of the compiler that **built** SIMORPH (captured at
    /// build time; `rustc_version` is whatever is on the PATH at run time,
    /// which may differ or be absent in a deployed binary).
    #[serde(default)]
    pub build_rustc_version: Option<String>,
    /// Build profile (`debug` / `release`).
    #[serde(default)]
    pub build_profile: Option<String>,
    /// Crate / application that requested the capture (`crate_name` is
    /// always `simorph-provenance`, the crate doing the capture).
    #[serde(default)]
    pub caller: Option<(String, String)>,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

impl EnvironmentCapture {
    /// Capture the current environment.  Shelling out to `rustc`/`cargo`/
    /// `git` is best-effort: missing tools yield `None`, never an error.
    pub fn capture() -> Self {
        Self {
            rustc_version: run("rustc", &["--version"]),
            cargo_version: run("cargo", &["--version"]),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            crate_name: env!("CARGO_PKG_NAME").to_string(),
            crate_version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: run("git", &["rev-parse", "HEAD"]),
            extra_versions: vec![],
            build_rustc_version: Some(env!("SIMORPH_BUILD_RUSTC").to_string())
                .filter(|s| !s.is_empty()),
            build_profile: Some(env!("SIMORPH_BUILD_PROFILE").to_string())
                .filter(|s| !s.is_empty()),
            caller: None,
        }
    }

    /// Capture on behalf of a calling crate, e.g.
    /// `EnvironmentCapture::capture_for(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))`.
    pub fn capture_for(crate_name: &str, version: &str) -> Self {
        let mut c = Self::capture();
        c.caller = Some((crate_name.to_string(), version.to_string()));
        c
    }

    pub fn with_version(mut self, name: &str, version: &str) -> Self {
        self.extra_versions
            .push((name.to_string(), version.to_string()));
        self
    }
}
