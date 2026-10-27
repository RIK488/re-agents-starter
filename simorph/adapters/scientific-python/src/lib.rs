//! # simorph-adapter-scientific-python
//!
//! Reuse instead of rewrite: mature open-source scientific engines exposed
//! as SIMORPH [`Component`]s.  Each engine is a server script in
//! `adapters/python/simorph_scientific/` speaking the JSON-lines protocol
//! (`adapters/PROTOCOL.md`), run by [`PythonComponent`] (`python -I`, scrubbed
//! environment, wall-clock timeout, output cap; see `simorph-security`).
//!
//! | Component name               | Upstream engine                        | Licence      |
//! |------------------------------|----------------------------------------|--------------|
//! | `scipy.integrate.solve_ivp`  | SciPy `solve_ivp` (RK45, DOP853, Radau, BDF, LSODA) + SymPy Jacobian | BSD-3-Clause |
//! | `scipy.optimize`             | SciPy `minimize`, `differential_evolution`, `linprog` (HiGHS) | BSD-3-Clause |
//! | `scipy.stats.qmc.moments`    | SciPy QMC (scrambled Sobol, Halton, LHS) | BSD-3-Clause |
//! | `sympy.dsolve`               | SymPy `dsolve` + `checkodesol`          | BSD-3-Clause |
//! | `mujoco.simulation`          | MuJoCo (Google DeepMind)                | Apache-2.0   |
//! | `gymnasium.episode`          | Gymnasium classic control (Farama)      | MIT          |
//! | `pytorch.mlp_regression`     | PyTorch                                 | BSD-3-Clause |
//!
//! Expressions supplied by users or agents (ODE right-hand sides,
//! objectives, constraints) are parsed by a whitelist AST compiler
//! (`simorph_scientific/common.py`) — never by `eval`/`sympify`.
//!
//! Availability is detected at run time: [`scientific_components`] returns,
//! for each engine, either the live component or the precise reason it is
//! unavailable (missing module), so nothing is ever faked.

use serde::{Deserialize, Serialize};
use simorph_adapter_python_bridge::{python_interpreter, python_module_available, PythonComponent};
use simorph_core::{Component, Result, SimorphError};
use simorph_security::SandboxPolicy;
use std::path::PathBuf;
use std::sync::Arc;

/// Static description of one wrapped engine.
#[derive(Debug, Clone, Copy)]
pub struct EngineSpec {
    /// SIMORPH component name (as declared in the script's manifest).
    pub name: &'static str,
    /// Script file in `adapters/python/simorph_scientific/`.
    pub script: &'static str,
    /// Python modules that must import for the component to be usable.
    pub modules: &'static [&'static str],
    /// Wall-clock limit per request (seconds).
    pub timeout_s: f64,
}

/// Every engine wrapped by this crate.
pub const ENGINES: &[EngineSpec] = &[
    EngineSpec {
        name: "scipy.integrate.solve_ivp",
        script: "scipy_ivp.py",
        modules: &["numpy", "scipy", "sympy"],
        timeout_s: 300.0,
    },
    EngineSpec {
        name: "scipy.optimize",
        script: "scipy_optimize.py",
        modules: &["numpy", "scipy", "sympy"],
        timeout_s: 300.0,
    },
    EngineSpec {
        name: "scipy.stats.qmc.moments",
        script: "scipy_qmc.py",
        modules: &["numpy", "scipy", "sympy"],
        timeout_s: 120.0,
    },
    EngineSpec {
        name: "sympy.dsolve",
        script: "sympy_dsolve.py",
        modules: &["sympy"],
        timeout_s: 120.0,
    },
    EngineSpec {
        name: "mujoco.simulation",
        script: "mujoco_sim.py",
        modules: &["numpy", "mujoco", "sympy"],
        timeout_s: 300.0,
    },
    EngineSpec {
        name: "gymnasium.episode",
        script: "gym_env.py",
        modules: &["numpy", "gymnasium", "sympy"],
        timeout_s: 120.0,
    },
    EngineSpec {
        name: "pytorch.mlp_regression",
        script: "torch_mlp.py",
        modules: &["torch", "sympy"],
        timeout_s: 600.0,
    },
];

/// Directory holding the engine scripts.
pub fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../python/simorph_scientific")
}

/// Sandbox policy used for an engine.  The scrubbed environment keeps only
/// `PATH`, `LANG`, `LC_ALL` and `PYTHONHASHSEED`; none of the wrapped
/// libraries needs more for headless use (MuJoCo is used without
/// rendering, so no `MUJOCO_GL`/display variables are passed; PyTorch runs
/// single-threaded on CPU and needs no `HOME` for this use).
pub fn policy(spec: &EngineSpec) -> SandboxPolicy {
    SandboxPolicy::strict()
        .with_timeout(spec.timeout_s)
        .with_max_output(64 * 1024 * 1024)
}

/// Find an engine spec by component name.
pub fn engine(name: &str) -> Option<&'static EngineSpec> {
    ENGINES.iter().find(|e| e.name == name)
}

/// Check that every module an engine needs imports with the selected
/// interpreter; returns the reason otherwise.
pub fn check_available(spec: &EngineSpec) -> std::result::Result<(), String> {
    for m in spec.modules {
        python_module_available(m)?;
    }
    Ok(())
}

/// Start one engine.  Fails with `NotImplemented` (and the reason) when a
/// required module is missing.
pub fn spawn(name: &str) -> Result<Arc<dyn Component>> {
    let spec = engine(name)
        .ok_or_else(|| SimorphError::NotFound(format!("scientific engine `{name}`")))?;
    if let Err(reason) = check_available(spec) {
        return Err(SimorphError::NotImplemented(format!(
            "{name} unavailable with interpreter {}: {reason}",
            python_interpreter()
        )));
    }
    let c = PythonComponent::from_script(scripts_dir().join(spec.script), policy(spec))?;
    if c.manifest().name != spec.name {
        return Err(SimorphError::Component {
            component: spec.name.into(),
            message: format!("script declares `{}`", c.manifest().name),
        });
    }
    Ok(Arc::new(c))
}

/// Every engine, live or with the reason it is unavailable.
pub fn scientific_components() -> Vec<(String, Result<Arc<dyn Component>>)> {
    ENGINES
        .iter()
        .map(|e| (e.name.to_string(), spawn(e.name)))
        .collect()
}

/// Detected version of one Python library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryStatus {
    pub module: String,
    pub version: Option<String>,
    pub error: Option<String>,
}

/// Versions of the scientific libraries visible to the interpreter.
pub fn capability_report() -> Vec<LibraryStatus> {
    ["numpy", "scipy", "sympy", "mujoco", "gymnasium", "torch"]
        .iter()
        .map(|m| match python_module_available(m) {
            Ok(v) => LibraryStatus {
                module: m.to_string(),
                version: Some(v),
                error: None,
            },
            Err(e) => LibraryStatus {
                module: m.to_string(),
                version: None,
                error: Some(e),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_exist_for_every_engine() {
        for e in ENGINES {
            assert!(scripts_dir().join(e.script).is_file(), "{}", e.script);
        }
        assert!(engine("nope").is_none());
        assert!(matches!(spawn("nope"), Err(SimorphError::NotFound(_))));
    }

    #[test]
    fn report_lists_every_library() {
        let r = capability_report();
        assert_eq!(r.len(), 6);
        for s in &r {
            assert!(s.version.is_some() ^ s.error.is_some(), "{s:?}");
            eprintln!("{} -> {:?} {:?}", s.module, s.version, s.error);
        }
    }
}
