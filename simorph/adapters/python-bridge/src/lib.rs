//! # simorph-adapter-python-bridge
//!
//! [`PythonComponent`] runs a Python script that uses the `simorph_adapter`
//! package (`adapters/python`) as an out-of-process component.  The
//! interpreter is started with `-I` (isolated mode: no user site-packages,
//! no `PYTHONPATH`, script directory not on `sys.path`) under the sandbox
//! policy enforced by `simorph_execution::SubprocessComponent`.
//!
//! The bridge is deliberately process-based rather than PyO3-based: a crash
//! or memory blow-up in a scientific library cannot take the SIMORPH kernel
//! down, cancellation is a process kill, and the policy's environment
//! scrubbing applies.  An in-process PyO3 path can be added later for
//! latency-critical kernels; it would bypass isolation and must be opt-in.

use simorph_core::{
    Component, ComponentManifest, ExecutionControl, Result, SimorphError, TaskRequest, TaskResult,
};
use simorph_execution::SubprocessComponent;
use simorph_security::SandboxPolicy;
use std::path::{Path, PathBuf};

/// A Python component.
#[derive(Debug)]
pub struct PythonComponent {
    inner: SubprocessComponent,
    script: PathBuf,
}

impl PythonComponent {
    /// Spawn `python3 -I <script>` (plus `extra_args`) and fetch its manifest.
    pub fn from_script(script: impl AsRef<Path>, policy: SandboxPolicy) -> Result<Self> {
        Self::from_script_with(python_interpreter(), script, &[], policy)
    }

    pub fn from_script_with(
        interpreter: impl Into<String>,
        script: impl AsRef<Path>,
        extra_args: &[&str],
        policy: SandboxPolicy,
    ) -> Result<Self> {
        let script = script.as_ref().to_path_buf();
        if !script.is_file() {
            return Err(SimorphError::NotFound(format!(
                "python script {}",
                script.display()
            )));
        }
        let mut args = vec!["-I".to_string(), script.to_string_lossy().to_string()];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        let inner = SubprocessComponent::new(interpreter, args, policy)?;
        Ok(Self { inner, script })
    }

    pub fn script(&self) -> &Path {
        &self.script
    }

    pub fn inner(&self) -> &SubprocessComponent {
        &self.inner
    }

    pub fn shutdown(&self) {
        self.inner.shutdown()
    }
}

impl Component for PythonComponent {
    fn manifest(&self) -> &ComponentManifest {
        self.inner.manifest()
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        self.inner.execute(req, ctl)
    }
}

/// Interpreter to use: `SIMORPH_PYTHON` if set, else `python3`.
pub fn python_interpreter() -> String {
    std::env::var("SIMORPH_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

/// Check that the interpreter exists and that `numpy` imports.  Returns a
/// human-readable reason on failure so tests can skip with a message.
pub fn python_with_numpy_available() -> std::result::Result<(), String> {
    let out = std::process::Command::new(python_interpreter())
        .args(["-I", "-c", "import numpy; print(numpy.__version__)"])
        .output()
        .map_err(|e| format!("cannot run {}: {e}", python_interpreter()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "numpy unavailable: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Path of the bundled `adapters/python` directory (repository checkout).
pub fn bundled_python_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("python")
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::run_component;

    fn policy() -> SandboxPolicy {
        SandboxPolicy::strict().with_timeout(60.0)
    }

    #[test]
    fn numpy_linear_solver_end_to_end() {
        if let Err(reason) = python_with_numpy_available() {
            eprintln!("SKIPPED numpy_linear_solver_end_to_end: {reason}");
            return;
        }
        let script = bundled_python_dir()
            .join("examples")
            .join("numpy_linear_solver.py");
        let c = PythonComponent::from_script(&script, policy()).unwrap();
        assert_eq!(c.manifest().name, "python.numpy.linear_solver");
        assert_eq!(c.manifest().units["x"], "1");
        // [[2,1],[1,3]] x = [3,5]  ->  x = [0.8, 1.4]
        let req = TaskRequest::new(c.manifest().id)
            .input("a", serde_json::json!([2.0, 1.0, 1.0, 3.0]))
            .input("b", serde_json::json!([3.0, 5.0]));
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        let x = r.get_vec_f64("x").unwrap();
        assert!(
            (x[0] - 0.8).abs() < 1e-12 && (x[1] - 1.4).abs() < 1e-12,
            "{x:?}"
        );
        assert!(r.get_f64("residual").unwrap() < 1e-12);
        assert_eq!(r.evidence, simorph_core::EvidenceLevel::Computed);
        // singular matrix -> component error, process survives
        let bad = TaskRequest::new(c.manifest().id)
            .input("a", serde_json::json!([1.0, 2.0, 2.0, 4.0]))
            .input("b", serde_json::json!([1.0, 1.0]));
        assert!(matches!(
            run_component(&c, &bad, &ExecutionControl::default()),
            Err(SimorphError::Component { .. })
        ));
        let r2 = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        assert_eq!(r2.get_vec_f64("x").unwrap(), x);
        c.shutdown();
    }

    #[test]
    fn linear_ode_rotation() {
        if let Err(reason) = python_with_numpy_available() {
            eprintln!("SKIPPED linear_ode_rotation: {reason}");
            return;
        }
        let script = bundled_python_dir()
            .join("examples")
            .join("scipy_ode_component.py");
        let c = PythonComponent::from_script(&script, policy()).unwrap();
        // dx/dt = [[0,1],[-1,0]] x : rotation; after t = pi/2, (1,0) -> (0,-1)
        let req = TaskRequest::new(c.manifest().id)
            .input("matrix", serde_json::json!([0.0, 1.0, -1.0, 0.0]))
            .input("x0", serde_json::json!([1.0, 0.0]))
            .input("t_end", std::f64::consts::FRAC_PI_2);
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        let x = r.get_vec_f64("x_end").unwrap();
        assert!(x[0].abs() < 1e-6 && (x[1] + 1.0).abs() < 1e-6, "{x:?}");
        assert!(r.metrics.contains_key("backend_scipy"));
        assert_eq!(r.evidence, simorph_core::EvidenceLevel::Simulated);
        c.shutdown();
    }

    #[test]
    fn missing_script_is_not_found() {
        assert!(matches!(
            PythonComponent::from_script("/nonexistent/x.py", policy()),
            Err(SimorphError::NotFound(_))
        ));
    }
}
