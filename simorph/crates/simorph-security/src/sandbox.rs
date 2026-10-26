//! Declarative sandbox policy for generated or external code.
//!
//! What this crate enforces itself (through `simorph-execution`): wall-clock
//! timeout (process kill), output size cap, environment scrubbing (only
//! allow-listed variables are passed), working directory.
//!
//! What must be enforced by the deployment layer and is *not* claimed here:
//! network isolation, filesystem read-only/writable mounts, CPU and memory
//! limits.  In production these are mapped onto container runtimes
//! (`--network none`, bind mounts, cgroup limits) or Wasm sandboxes; the
//! [`SandboxPolicy::to_container_args`] helper produces the corresponding
//! OCI runtime flags so that the same policy object drives both layers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SandboxPolicy {
    /// Whether network egress is allowed.
    pub network: bool,
    pub writable_paths: Vec<PathBuf>,
    pub read_only_paths: Vec<PathBuf>,
    /// CPU time limit in seconds (0 = unlimited).  Enforced as wall-clock
    /// timeout by the subprocess runner.
    pub max_cpu_s: f64,
    /// Memory limit in MB (0 = unlimited; container layer only).
    pub max_memory_mb: u64,
    /// Maximum bytes read from the child's stdout before it is killed.
    pub max_output_bytes: u64,
    /// Environment variables passed through to the child.
    pub env_allowlist: Vec<String>,
    /// Working directory for the child (must be inside `writable_paths`
    /// when the latter is non-empty).
    pub working_dir: Option<PathBuf>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

impl SandboxPolicy {
    /// No network, no secrets, 60 s, 16 MiB output, minimal environment.
    pub fn strict() -> Self {
        Self {
            network: false,
            writable_paths: vec![],
            read_only_paths: vec![],
            max_cpu_s: 60.0,
            max_memory_mb: 2048,
            max_output_bytes: 16 * 1024 * 1024,
            env_allowlist: vec![
                "PATH".into(),
                "LANG".into(),
                "LC_ALL".into(),
                "PYTHONHASHSEED".into(),
            ],
            working_dir: None,
        }
    }

    pub fn with_timeout(mut self, s: f64) -> Self {
        self.max_cpu_s = s;
        self
    }
    pub fn with_working_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(p.into());
        self
    }
    pub fn with_env(mut self, names: &[&str]) -> Self {
        self.env_allowlist
            .extend(names.iter().map(|s| s.to_string()));
        self
    }
    pub fn with_max_output(mut self, bytes: u64) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// Structural validation.
    pub fn validate(&self) -> Result<()> {
        if self.max_cpu_s < 0.0 {
            return Err(SimorphError::InvalidInput("max_cpu_s must be >= 0".into()));
        }
        if self.max_output_bytes == 0 {
            return Err(SimorphError::InvalidInput(
                "max_output_bytes must be > 0".into(),
            ));
        }
        for name in &self.env_allowlist {
            if name.is_empty() || name.contains('=') {
                return Err(SimorphError::InvalidInput(format!(
                    "invalid env name `{name}`"
                )));
            }
        }
        if let Some(wd) = &self.working_dir {
            if !self.writable_paths.is_empty()
                && !self.writable_paths.iter().any(|w| wd.starts_with(w))
            {
                return Err(SimorphError::InvalidInput(
                    "working_dir must be inside one of writable_paths".into(),
                ));
            }
        }
        Ok(())
    }

    /// Environment for the child process: only allow-listed variables that
    /// exist in the parent environment.  Secrets never leak unless
    /// explicitly allow-listed.
    pub fn to_subprocess_env(&self) -> BTreeMap<String, String> {
        self.to_subprocess_env_from(std::env::vars())
    }

    pub fn to_subprocess_env_from(
        &self,
        parent: impl IntoIterator<Item = (String, String)>,
    ) -> BTreeMap<String, String> {
        let parent: BTreeMap<String, String> = parent.into_iter().collect();
        let mut out = BTreeMap::new();
        for name in &self.env_allowlist {
            if let Some(v) = parent.get(name) {
                out.insert(name.clone(), v.clone());
            }
        }
        out
    }

    /// OCI-style runtime flags expressing the parts of the policy the
    /// subprocess runner cannot enforce itself.
    pub fn to_container_args(&self) -> Vec<String> {
        let mut args = vec![];
        if !self.network {
            args.push("--network=none".into());
        }
        if self.max_memory_mb > 0 {
            args.push(format!("--memory={}m", self.max_memory_mb));
        }
        for p in &self.read_only_paths {
            args.push(format!("--mount=type=bind,src={0},dst={0},ro", p.display()));
        }
        for p in &self.writable_paths {
            args.push(format!("--mount=type=bind,src={0},dst={0}", p.display()));
        }
        args.push("--read-only".into());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_is_scrubbed_to_allowlist() {
        let p = SandboxPolicy::strict().with_env(&["SIMORPH_SEED"]);
        let parent = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), "hunter2".to_string()),
            ("SIMORPH_SEED".to_string(), "42".to_string()),
        ];
        let env = p.to_subprocess_env_from(parent);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
        assert_eq!(env.get("SIMORPH_SEED").map(String::as_str), Some("42"));
        assert!(!env.contains_key("AWS_SECRET_ACCESS_KEY"));
    }

    #[test]
    fn validation_catches_bad_values() {
        assert!(SandboxPolicy::strict().validate().is_ok());
        assert!(SandboxPolicy::strict()
            .with_max_output(0)
            .validate()
            .is_err());
        let mut p = SandboxPolicy::strict();
        p.env_allowlist.push("A=B".into());
        assert!(p.validate().is_err());
        let mut p = SandboxPolicy::strict().with_working_dir("/tmp/x");
        p.writable_paths = vec![PathBuf::from("/data")];
        assert!(p.validate().is_err());
    }

    #[test]
    fn container_args_reflect_policy() {
        let p = SandboxPolicy::strict();
        let a = p.to_container_args();
        assert!(a.contains(&"--network=none".to_string()));
        assert!(a.iter().any(|s| s.starts_with("--memory=")));
    }
}
