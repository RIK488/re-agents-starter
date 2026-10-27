//! Expose any [`DynamicalModel`] through the universal `Component` contract.

use crate::integrators::{Solver, SolverOptions};
use crate::model::{DynamicalModel, Params};
use crate::simulate::{simulate, Trajectory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simorph_core::{
    Capability, Component, ComponentManifest, EvidenceLevel, ExecutionControl, Port, ReferenceCase,
    Result, TaskRequest, TaskResult, UncertaintyModel, ValidationProtocol,
};
use std::sync::Arc;

/// Optional per-request configuration accepted in `TaskRequest::config`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OdeRunConfig {
    /// Initial state override (state order).
    #[serde(default)]
    pub x0: Option<Vec<f64>>,
    #[serde(default)]
    pub t0: Option<f64>,
    #[serde(default)]
    pub solver: Option<String>,
    #[serde(default)]
    pub dt: Option<f64>,
    #[serde(default)]
    pub rtol: Option<f64>,
    #[serde(default)]
    pub atol: Option<f64>,
    #[serde(default)]
    pub dt_max: Option<f64>,
    /// Maximum number of trajectory samples returned (down-sampled uniformly).
    #[serde(default)]
    pub max_points: Option<usize>,
}

/// Generic ODE component.
pub struct OdeComponent {
    model: Arc<dyn DynamicalModel>,
    manifest: ComponentManifest,
    defaults: SolverOptions,
}

impl OdeComponent {
    pub fn new(model: Arc<dyn DynamicalModel>, version: &str, provider: &str) -> Self {
        Self::with_defaults(model, version, provider, SolverOptions::default())
    }

    pub fn with_defaults(
        model: Arc<dyn DynamicalModel>,
        version: &str,
        provider: &str,
        defaults: SolverOptions,
    ) -> Self {
        let mut inputs: Vec<Port> = model
            .parameter_names()
            .into_iter()
            .map(|(n, u)| Port::scalar(n, u))
            .collect();
        inputs.push(Port::scalar("t_end", "s").with_range(0.0, f64::INFINITY));
        let mut outputs: Vec<Port> = model
            .state_names()
            .into_iter()
            .map(|(n, u)| Port::scalar(format!("final_{n}"), u))
            .collect();
        outputs.push(Port::scalar("t_final", "s"));
        for (n, u) in model.output_names() {
            outputs.push(Port::scalar(n, u));
        }
        let mut manifest = ComponentManifest::in_process(model.name(), version, provider)
            .with_capabilities(vec![Capability::OdeIntegration])
            .with_inputs(inputs)
            .with_outputs(outputs)
            .with_validation(ValidationProtocol {
                reference_cases: model.reference_cases(),
                invariants: vec![],
                convergence_order: Some(defaults.solver.order() as f64),
            });
        manifest.supported_domains = model.domains();
        manifest.assumptions = model.assumptions();
        for (p, (lo, hi)) in model.validity_ranges() {
            manifest.validity_domain.ranges.insert(p, (lo, hi));
        }
        // rtol is a *local* (per-step) tolerance of the adaptive method; it
        // does not bound the global error, which may be much larger over long
        // horizons (audit: rtol = 1e-8 gave a global error of 2.3e-5).
        manifest.uncertainty_model = UncertaintyModel::Described {
            description: format!(
                "local error tolerance rtol = {:e} per step (adaptive methods); global error not bounded; fixed-step methods have no error estimate",
                defaults.rtol
            ),
        };
        Self {
            model,
            manifest,
            defaults,
        }
    }

    pub fn model(&self) -> &Arc<dyn DynamicalModel> {
        &self.model
    }

    pub fn add_reference_case(&mut self, case: ReferenceCase) {
        self.manifest.validation_protocol.reference_cases.push(case);
    }

    fn options(&self, req: &TaskRequest) -> Result<(SolverOptions, OdeRunConfig)> {
        let cfg: OdeRunConfig = if req.config.is_null() {
            OdeRunConfig::default()
        } else {
            serde_json::from_value(req.config.clone())?
        };
        let mut o = self.defaults.clone();
        if let Some(s) = &cfg.solver {
            o.solver = Solver::parse(s)?;
        }
        if let Some(v) = cfg.dt {
            o.dt = v;
        }
        if let Some(v) = cfg.rtol {
            o.rtol = v;
        }
        if let Some(v) = cfg.atol {
            o.atol = v;
        }
        if let Some(v) = cfg.dt_max {
            o.dt_max = v;
        }
        // Fidelity < 1 loosens tolerances / enlarges the fixed step (§8):
        // fidelity 0 -> tolerances ×1000, step ×10.
        let f = req.fidelity.clamp(0.0, 1.0);
        if f < 1.0 {
            let loosen = 10f64.powf(3.0 * (1.0 - f));
            o.rtol *= loosen;
            o.atol *= loosen;
            if !o.solver.is_adaptive() {
                o.dt *= 10f64.powf(1.0 - f);
            }
        }
        Ok((o, cfg))
    }

    /// Run and return the raw trajectory (useful to callers inside Rust).
    pub fn run_trajectory(
        &self,
        req: &TaskRequest,
        ctl: &ExecutionControl,
    ) -> Result<(Trajectory, OdeRunConfig)> {
        let (opts, cfg) = self.options(req)?;
        let mut params = Params::new();
        for (name, _) in self.model.parameter_names() {
            params.insert(name.clone(), req.get_f64(&name)?);
        }
        let t_end = req.get_f64("t_end")?;
        let t0 = cfg.t0.unwrap_or(0.0);
        let traj = simulate(
            self.model.as_ref(),
            &params,
            cfg.x0.as_deref(),
            (t0, t_end),
            &opts,
            ctl,
        )?;
        Ok((traj, cfg))
    }
}

fn downsample(traj: &Trajectory, max_points: usize) -> Vec<usize> {
    let n = traj.len();
    if max_points == 0 || n <= max_points {
        return (0..n).collect();
    }
    let mut idx: Vec<usize> = (0..max_points)
        .map(|k| (k as f64 * (n - 1) as f64 / (max_points - 1) as f64).round() as usize)
        .collect();
    idx.dedup();
    idx
}

impl Component for OdeComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }

    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let (traj, cfg) = self.run_trajectory(req, ctl)?;
        let mut res = TaskResult::new(req, EvidenceLevel::Simulated);
        let xf = traj.final_state();
        for (i, (n, _)) in traj.state_names.iter().enumerate() {
            res.outputs.insert(format!("final_{n}"), json!(xf[i]));
        }
        res.outputs
            .insert("t_final".into(), json!(traj.final_time()));
        for (k, series) in &traj.outputs {
            if let Some(v) = series.last() {
                res.outputs.insert(k.clone(), json!(v));
            }
        }
        let idx = downsample(&traj, cfg.max_points.unwrap_or(1000));
        let times: Vec<f64> = idx.iter().map(|&i| traj.times[i]).collect();
        let mut states = serde_json::Map::new();
        for (j, (n, _)) in traj.state_names.iter().enumerate() {
            states.insert(
                n.clone(),
                json!(idx.iter().map(|&i| traj.states[i][j]).collect::<Vec<_>>()),
            );
        }
        let mut outs = serde_json::Map::new();
        for (k, series) in &traj.outputs {
            outs.insert(
                k.clone(),
                json!(idx.iter().map(|&i| series[i]).collect::<Vec<_>>()),
            );
        }
        res.outputs.insert(
            "trajectory".into(),
            json!({ "times": times, "states": Value::Object(states), "outputs": Value::Object(outs) }),
        );
        res.outputs.insert(
            "events".into(),
            json!(traj
                .events
                .iter()
                .map(|e| json!({"index": e.index, "time": e.time}))
                .collect::<Vec<_>>()),
        );
        let s = &traj.stats;
        res.metrics.insert("steps".into(), s.steps as f64);
        res.metrics
            .insert("rejected_steps".into(), s.rejected_steps as f64);
        res.metrics.insert("rhs_evals".into(), s.rhs_evals as f64);
        res.metrics
            .insert("newton_iters".into(), s.newton_iters as f64);
        // Only adaptive methods have a local error estimate; for fixed-step
        // methods the metric is omitted rather than reported as 0.
        if s.error_estimated {
            res.metrics
                .insert("max_error_estimate".into(), s.max_error_estimate);
        }
        res.metrics.insert(
            "error_estimated".into(),
            if s.error_estimated { 1.0 } else { 0.0 },
        );
        res.metrics.insert("events".into(), s.events as f64);
        if traj.terminated_by_event {
            res.warnings
                .push(format!("terminated by event at t={}", traj.final_time()));
        }
        // Validity-domain excursions along the trajectory (states named like
        // a validity range) are reported as warnings, never silently.
        for (name, (lo, hi)) in &self.manifest.validity_domain.ranges {
            if let Some(j) = traj.state_index(name) {
                if let Some(t) = traj
                    .times
                    .iter()
                    .zip(&traj.states)
                    .find(|(_, s)| s[j] < *lo || s[j] > *hi)
                    .map(|(t, _)| *t)
                {
                    res.warnings.push(format!(
                        "state `{name}` left validity domain [{lo}, {hi}] at t={t}"
                    ));
                }
            }
        }
        if req.fidelity < 1.0 {
            res.warnings.push(format!(
                "reduced fidelity {} (tolerances loosened)",
                req.fidelity
            ));
        }
        Ok(res)
    }
}
