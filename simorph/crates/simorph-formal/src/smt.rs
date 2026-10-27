//! Z3 backend (SMT-LIB 2 through the sandboxed helper).

use crate::runner::{run_tool, RunInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use simorph_security::SandboxPolicy;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SatResult {
    Sat { model: BTreeMap<String, ModelValue> },
    Unsat,
    Unknown { reason: String },
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelValue {
    /// Exact value as printed by the solver (e.g. `1/65536`).
    pub exact: String,
    pub approx: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolverRun {
    pub solver: String,
    pub version: String,
    pub result: SatResult,
    pub solver_ms: f64,
    pub info: RunInfo,
}

/// Run an SMT-LIB script with Z3; `declared` restricts the reported model.
pub fn run_z3(
    smtlib: &str,
    declared: &[String],
    timeout_ms: u64,
    policy: &SandboxPolicy,
) -> Result<SolverRun> {
    let req = serde_json::json!({"op": "z3", "smtlib": smtlib, "declared": declared, "timeout_ms": timeout_ms});
    let (v, info) = match run_tool(&req, policy) {
        Ok(x) => x,
        Err(SimorphError::Timeout(ms)) => {
            return Ok(SolverRun {
                solver: "z3".into(),
                version: "?".into(),
                result: SatResult::Timeout,
                solver_ms: ms as f64,
                info: RunInfo {
                    interpreter: simorph_adapter_python_bridge::python_interpreter(),
                    network_isolated: false,
                    wall_ms: ms as f64,
                },
            })
        }
        Err(e) => return Err(e),
    };
    let version = v["version"].as_str().unwrap_or("?").to_string();
    let solver_ms = v["elapsed_ms"].as_f64().unwrap_or(f64::NAN);
    let result = match v["result"].as_str() {
        Some("unsat") => SatResult::Unsat,
        Some("sat") => {
            let mut model = BTreeMap::new();
            if let Some(m) = v["model"].as_object() {
                for (k, e) in m {
                    model.insert(
                        k.clone(),
                        ModelValue {
                            exact: e["exact"].as_str().unwrap_or("").to_string(),
                            approx: e["approx"].as_f64().unwrap_or(f64::NAN),
                        },
                    );
                }
            }
            SatResult::Sat { model }
        }
        Some("unknown") => {
            let reason = v["reason"].as_str().unwrap_or("").to_string();
            if reason.contains("timeout") || reason.contains("canceled") {
                SatResult::Timeout
            } else {
                SatResult::Unknown { reason }
            }
        }
        other => {
            return Err(SimorphError::Component {
                component: "z3".into(),
                message: format!("unexpected result {other:?}"),
            })
        }
    };
    Ok(SolverRun {
        solver: "z3".into(),
        version,
        result,
        solver_ms,
        info,
    })
}
