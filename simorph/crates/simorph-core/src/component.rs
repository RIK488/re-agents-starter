//! Universal execution interface (§7).
//!
//! The ten operations required by the specification are modelled as:
//!
//! | Operation            | Where                                           |
//! |----------------------|-------------------------------------------------|
//! | discover             | `ComponentRegistry` (model-registry crate)      |
//! | check compatibility  | [`Component::check_compatibility`]              |
//! | prepare environment  | [`Component::prepare`]                          |
//! | submit task          | [`Component::execute`]                          |
//! | control execution    | [`CancellationToken`] + `Deadline`               |
//! | collect results      | [`TaskResult`]                                  |
//! | cancel               | [`CancellationToken::cancel`]                   |
//! | reproduce            | `TaskRequest::seed` + `TaskResult::provenance`  |
//! | validate outputs     | validation crate reads `ComponentManifest::validation_protocol` |
//! | publish metrics      | [`TaskResult::metrics`]                         |

use crate::error::Result;
use crate::ids::{ComponentId, RunId};
use crate::manifest::ComponentManifest;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cooperative cancellation token shared between orchestrator and component.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
    /// Convenience: returns `Err(Cancelled)` if cancelled.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(crate::SimorphError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Wall-clock deadline.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    pub at: Instant,
}

impl Deadline {
    pub fn after(d: Duration) -> Self {
        Self {
            at: Instant::now() + d,
        }
    }
    pub fn expired(&self) -> bool {
        Instant::now() >= self.at
    }
    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }
}

/// Execution control passed to every component invocation.
#[derive(Debug, Clone)]
pub struct ExecutionControl {
    pub cancel: CancellationToken,
    pub deadline: Option<Deadline>,
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self {
            cancel: CancellationToken::new(),
            deadline: None,
        }
    }
}

impl ExecutionControl {
    pub fn with_timeout(d: Duration) -> Self {
        Self {
            cancel: CancellationToken::new(),
            deadline: Some(Deadline::after(d)),
        }
    }
    /// Check both cancellation and deadline.
    pub fn check(&self) -> Result<()> {
        self.cancel.check()?;
        if let Some(d) = self.deadline {
            if d.expired() {
                return Err(crate::SimorphError::Timeout(0));
            }
        }
        Ok(())
    }
}

/// Inputs of a task: named values (JSON) + numeric seed + fidelity hint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskRequest {
    pub run_id: RunId,
    pub component: ComponentId,
    /// Named inputs; values follow the component's `input_schema`.
    pub inputs: IndexMap<String, serde_json::Value>,
    /// Seed for deterministic reproduction.  `None` means "component default".
    #[serde(default)]
    pub seed: Option<u64>,
    /// Fidelity level requested by uncertainty-guided simulation (§8).
    /// 0 = cheapest approximation, 1 = full fidelity.
    #[serde(default = "full_fidelity")]
    pub fidelity: f64,
    /// Free-form configuration the component understands.
    #[serde(default)]
    pub config: serde_json::Value,
}

fn full_fidelity() -> f64 {
    1.0
}

impl TaskRequest {
    pub fn new(component: ComponentId) -> Self {
        Self {
            run_id: RunId::new(),
            component,
            inputs: IndexMap::new(),
            seed: None,
            fidelity: 1.0,
            config: serde_json::Value::Null,
        }
    }
    pub fn input(mut self, name: &str, v: impl Into<serde_json::Value>) -> Self {
        self.inputs.insert(name.to_string(), v.into());
        self
    }
    pub fn seed(mut self, s: u64) -> Self {
        self.seed = Some(s);
        self
    }
    pub fn fidelity(mut self, f: f64) -> Self {
        self.fidelity = f;
        self
    }
    pub fn config(mut self, c: serde_json::Value) -> Self {
        self.config = c;
        self
    }
    pub fn get_f64(&self, name: &str) -> Result<f64> {
        self.inputs
            .get(name)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                crate::SimorphError::InvalidInput(format!("missing numeric input `{name}`"))
            })
    }
    pub fn get_f64_or(&self, name: &str, default: f64) -> f64 {
        self.inputs
            .get(name)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    }
    pub fn get_vec_f64(&self, name: &str) -> Result<Vec<f64>> {
        let v = self
            .inputs
            .get(name)
            .ok_or_else(|| crate::SimorphError::InvalidInput(format!("missing input `{name}`")))?;
        v.as_array()
            .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
            .ok_or_else(|| {
                crate::SimorphError::InvalidInput(format!("input `{name}` is not an array"))
            })
    }
    /// Hash of everything that determines the output (for reproduction).
    pub fn content_hash(&self) -> String {
        #[derive(Serialize)]
        struct Key<'a> {
            component: &'a ComponentId,
            inputs: &'a IndexMap<String, serde_json::Value>,
            seed: Option<u64>,
            fidelity: f64,
            config: &'a serde_json::Value,
        }
        crate::hashing::hash_json(&Key {
            component: &self.component,
            inputs: &self.inputs,
            seed: self.seed,
            fidelity: self.fidelity,
            config: &self.config,
        })
        .expect("serialisable")
    }
}

/// Outputs of a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskResult {
    pub run_id: RunId,
    pub component: ComponentId,
    pub outputs: IndexMap<String, serde_json::Value>,
    /// Metrics published by the component (wall time, iterations, residuals…).
    #[serde(default)]
    pub metrics: IndexMap<String, f64>,
    /// Epistemic level the component itself claims for its outputs.  The
    /// validation engine may *lower* this; it can never be raised here.
    pub evidence: crate::evidence::EvidenceLevel,
    /// Hash of the request that produced this result.
    pub request_hash: String,
    /// Free-form warnings (e.g. "left validity domain at t=42s").
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub wall_time_ms: u64,
}

impl TaskResult {
    pub fn new(req: &TaskRequest, evidence: crate::evidence::EvidenceLevel) -> Self {
        Self {
            run_id: req.run_id,
            component: req.component,
            outputs: IndexMap::new(),
            metrics: IndexMap::new(),
            evidence,
            request_hash: req.content_hash(),
            warnings: vec![],
            wall_time_ms: 0,
        }
    }
    pub fn output(mut self, name: &str, v: impl Into<serde_json::Value>) -> Self {
        self.outputs.insert(name.to_string(), v.into());
        self
    }
    pub fn metric(mut self, name: &str, v: f64) -> Self {
        self.metrics.insert(name.to_string(), v);
        self
    }
    pub fn get_f64(&self, name: &str) -> Result<f64> {
        self.outputs
            .get(name)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                crate::SimorphError::InvalidInput(format!("missing numeric output `{name}`"))
            })
    }
    pub fn get_vec_f64(&self, name: &str) -> Result<Vec<f64>> {
        self.outputs
            .get(name)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
            .ok_or_else(|| {
                crate::SimorphError::InvalidInput(format!("missing array output `{name}`"))
            })
    }
    /// Hash of the outputs (used to check reproducibility).
    pub fn output_hash(&self) -> String {
        crate::hashing::hash_json(&self.outputs).expect("serialisable")
    }
}

/// Compatibility verdict returned by [`Component::check_compatibility`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Compatibility {
    pub compatible: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl Compatibility {
    pub fn ok() -> Self {
        Self {
            compatible: true,
            reasons: vec![],
        }
    }
    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            compatible: false,
            reasons: vec![reason.into()],
        }
    }
}

/// The universal component trait.  Implemented natively by Rust components
/// and by adapters wrapping Python, FMI, subprocess or HTTP tools.
pub trait Component: Send + Sync {
    fn manifest(&self) -> &ComponentManifest;

    /// Verify that a request can be executed: all required inputs present,
    /// inside the validity domain, resource needs satisfiable.
    fn check_compatibility(&self, req: &TaskRequest) -> Compatibility {
        let m = self.manifest();
        let mut reasons = vec![];
        for p in &m.input_schema {
            match req.inputs.get(&p.name) {
                None => reasons.push(format!("missing input `{}`", p.name)),
                Some(v) => {
                    if let (Some(x), Some((lo, hi))) = (v.as_f64(), p.range) {
                        if x < lo || x > hi {
                            reasons.push(format!(
                                "input `{}`={x} outside port range [{lo}, {hi}]",
                                p.name
                            ));
                        }
                    }
                    if let (Some(x), Some((lo, hi))) =
                        (v.as_f64(), m.validity_domain.ranges.get(&p.name))
                    {
                        if x < *lo || x > *hi {
                            reasons.push(format!(
                                "input `{}`={x} outside validity domain [{lo}, {hi}]",
                                p.name
                            ));
                        }
                    }
                }
            }
        }
        Compatibility {
            compatible: reasons.is_empty(),
            reasons,
        }
    }

    /// Prepare the execution environment (download, compile, warm caches).
    /// Default: nothing to do.
    fn prepare(&self) -> Result<()> {
        Ok(())
    }

    /// Execute a task synchronously.  Long computations must poll
    /// `ctl.check()` regularly to honour cancellation and deadlines.
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult>;
}

/// Helper: run a component with timing and compatibility checks.
pub fn run_component(
    c: &dyn Component,
    req: &TaskRequest,
    ctl: &ExecutionControl,
) -> Result<TaskResult> {
    let compat = c.check_compatibility(req);
    if !compat.compatible {
        return Err(crate::SimorphError::Incompatible(compat.reasons.join("; ")));
    }
    let start = Instant::now();
    let mut res = c.execute(req, ctl)?;
    res.wall_time_ms = start.elapsed().as_millis() as u64;
    res.metrics
        .entry("wall_time_ms".into())
        .or_insert(res.wall_time_ms as f64);
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceLevel;
    use crate::quantity::Port;

    struct Doubler {
        m: ComponentManifest,
    }
    impl Component for Doubler {
        fn manifest(&self) -> &ComponentManifest {
            &self.m
        }
        fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
            let x = req.get_f64("x")?;
            Ok(TaskResult::new(req, EvidenceLevel::Computed).output("y", 2.0 * x))
        }
    }

    #[test]
    fn compatibility_rejects_out_of_range_and_missing() {
        let d = Doubler {
            m: ComponentManifest::in_process("doubler", "1", "t")
                .with_inputs(vec![Port::scalar("x", "1").with_range(0.0, 10.0)])
                .with_outputs(vec![Port::scalar("y", "1")]),
        };
        let bad = TaskRequest::new(d.m.id).input("x", 100.0);
        assert!(!d.check_compatibility(&bad).compatible);
        let missing = TaskRequest::new(d.m.id);
        assert!(!d.check_compatibility(&missing).compatible);
        let good = TaskRequest::new(d.m.id).input("x", 3.0);
        let r = run_component(&d, &good, &ExecutionControl::default()).unwrap();
        assert_eq!(r.get_f64("y").unwrap(), 6.0);
        assert_eq!(r.request_hash, good.content_hash());
    }

    #[test]
    fn request_hash_depends_on_seed_and_fidelity() {
        let id = ComponentId::new();
        let a = TaskRequest::new(id).input("x", 1.0).seed(1);
        let b = TaskRequest::new(id).input("x", 1.0).seed(2);
        let c = TaskRequest::new(id).input("x", 1.0).seed(1).fidelity(0.5);
        assert_ne!(a.content_hash(), b.content_hash());
        assert_ne!(a.content_hash(), c.content_hash());
        // run_id does not affect the hash: same science, same hash.
        let a2 = TaskRequest {
            run_id: RunId::new(),
            ..a.clone()
        };
        assert_eq!(a.content_hash(), a2.content_hash());
    }

    #[test]
    fn cancellation_is_observed() {
        let ctl = ExecutionControl::default();
        assert!(ctl.check().is_ok());
        ctl.cancel.cancel();
        assert!(matches!(ctl.check(), Err(crate::SimorphError::Cancelled)));
    }
}
