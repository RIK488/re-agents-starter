//! # Campaign runtime
//!
//! Implements [`ExperimentExecutor`] for the scientific director with the
//! real SIMORPH engines:
//!
//! | Purpose          | Engine                                               |
//! |------------------|------------------------------------------------------|
//! | ReferenceCheck   | `simorph_validation::check_reference_cases` + invariants |
//! | Screening        | Latin-hypercube design (`simorph_experiment`) + local sensitivities (`simorph_uncertainty`) |
//! | Optimisation     | `MethodSelector` → chosen method (`simorph_optimization`) |
//! | Verification     | full-fidelity re-run of the retained design point     |
//! | Calibration      | z-test of predictions against supplied observations  |
//! | CounterExample   | `find_counter_example` on the hard constraints        |
//!
//! Every run is validated independently (invariants, evidence
//! classification) and recorded in the provenance ledger.  Success criteria
//! are evaluated with the validation engine's expression evaluator on
//! variables produced by the runtime (`reference_cases_failed`,
//! `failed_runs`, `best_objective_improved`,
//! `relative_change_vs_low_fidelity`, `z_score`, plus the outputs of the
//! retained run), so the criteria fixed at planning time are the ones that
//! decide.
//!
//! Unit handling: problem quantities are converted into each component's
//! port units with `simorph_units` before any request is built.

use indexmap::IndexMap;
use simorph_core::*;
use simorph_experiment::{
    find_counter_example, run_experiment, Design, ExperimentPlan, Factor, ProvenanceHook,
};
use simorph_optimization::{
    annealing, bayesian, evolutionary, gradient, nelder_mead,
    selector::{EvaluationCost, MethodSelector},
    Budget, ComponentProblem, Method,
};
use simorph_plugin_sdk::ComponentSet;
use simorph_provenance::{Ledger, ProvenanceKind};
use simorph_svc_scientific_director::{
    ExperimentExecutor, ExperimentOutcome, ExperimentPurpose, PlannedExperiment, RunSummary,
};
use simorph_validation::{
    checks, classify, evaluate, CheckResult, ReferenceOrigin, ValidationTarget, Value,
};

/// An observation supplied for calibration: inputs (problem units), the
/// observed output, its value and standard deviation (port units of the component).
#[derive(Debug, Clone)]
pub struct Observation {
    pub inputs: IndexMap<String, f64>,
    pub output: String,
    pub value: f64,
    pub std: f64,
}

pub struct CampaignRuntime {
    pub problem: ProblemSpec,
    pub components: ComponentSet,
    pub ledger: Ledger,
    pub observations: Vec<Observation>,
    pub seed: u64,
    /// Optional archive of experiment records (enables reproduction by id).
    pub archive: Option<simorph_experiment::ExperimentArchive>,
    /// Best scalarised objective seen during screening (per component).
    best_screening: IndexMap<ComponentId, (f64, IndexMap<String, f64>)>,
    /// Best design point of the optimisation (per component).
    best_optimised: IndexMap<ComponentId, (Vec<f64>, IndexMap<String, f64>)>,
    /// Method chosen by the selector (for the report).
    pub chosen_methods: IndexMap<ComponentId, (Method, Vec<String>)>,
}

impl CampaignRuntime {
    pub fn new(problem: ProblemSpec, components: ComponentSet, ledger: Ledger) -> Self {
        Self {
            problem,
            components,
            ledger,
            observations: vec![],
            seed: 42,
            archive: None,
            best_screening: IndexMap::new(),
            best_optimised: IndexMap::new(),
            chosen_methods: IndexMap::new(),
        }
    }

    fn component(&self, id: ComponentId) -> Result<simorph_plugin_sdk::SharedComponent> {
        self.components
            .get_by_id(id)
            .ok_or_else(|| SimorphError::NotFound(format!("component {id}")))
    }

    /// Unit of a problem quantity (known or design variable).
    fn problem_unit(&self, name: &str) -> Option<String> {
        self.problem
            .knowns
            .iter()
            .find(|q| q.name == name)
            .map(|q| q.unit.clone())
            .or_else(|| {
                self.problem
                    .design_variables
                    .iter()
                    .find(|d| d.name == name)
                    .map(|d| d.unit.clone())
            })
    }

    /// Convert a problem value into the component port unit.
    fn to_port_unit(&self, m: &ComponentManifest, name: &str, value: f64) -> Result<f64> {
        let port = m.input_schema.iter().find(|p| p.name == name);
        match (port, self.problem_unit(name)) {
            (Some(p), Some(u)) => simorph_units::convert(value, &u, &p.unit),
            _ => Ok(value),
        }
    }

    /// Base request holding the fixed inputs converted to port units.
    fn base_request(
        &self,
        m: &ComponentManifest,
        planned: &PlannedExperiment,
    ) -> Result<TaskRequest> {
        let mut req = TaskRequest::new(m.id).fidelity(planned.fidelity);
        for (k, v) in &planned.fixed_inputs {
            if m.input_schema.iter().any(|p| &p.name == k) {
                req.inputs.insert(
                    k.clone(),
                    serde_json::Value::from(self.to_port_unit(m, k, *v)?),
                );
            }
        }
        Ok(req)
    }

    fn factors(&self, m: &ComponentManifest, planned: &PlannedExperiment) -> Result<Vec<Factor>> {
        planned
            .design_variables
            .iter()
            .map(|d| {
                let unit = m
                    .input_schema
                    .iter()
                    .find(|p| p.name == d.name)
                    .map(|p| p.unit.clone())
                    .unwrap_or(d.unit.clone());
                Ok(Factor::new(
                    &d.name,
                    &unit,
                    self.to_port_unit(m, &d.name, d.low)?,
                    self.to_port_unit(m, &d.name, d.high)?,
                ))
            })
            .collect()
    }

    /// Validate one result independently and record it.  Returns the run
    /// summary with the evidence level *granted by the validator*.
    fn validate_run(
        &self,
        m: &ComponentManifest,
        inputs: IndexMap<String, f64>,
        res: &TaskResult,
        extra: Vec<CheckResult>,
    ) -> RunSummary {
        let mut checks = vec![checks::check_invariants(
            &m.validation_protocol.invariants,
            res,
        )];
        checks.extend(extra);
        let rec = classify(
            ValidationTarget::Run { run_id: res.run_id },
            res,
            checks,
            &[ReferenceOrigin::Analytic],
            None,
            None,
        );
        let _ = self.ledger.record(
            ProvenanceKind::Validation,
            format!("validation du run {}", res.run_id),
            &rec,
            &[],
        );
        // Outputs are reported in the *problem's* units (objectives, constraints,
        // unknowns) so that criteria and conclusions are expressed as the owner
        // declared them; raw port-unit values stay in the TaskResult (provenance).
        let outputs: IndexMap<String, f64> = res
            .outputs
            .iter()
            .filter_map(|(k, v)| {
                v.as_f64()
                    .map(|x| (k.clone(), self.to_problem_unit(m, k, x)))
            })
            .collect();
        RunSummary {
            run_id: res.run_id,
            inputs,
            outputs,
            evidence: rec.verdict,
            validation: Some(rec.id),
            valid: rec.all_passed(),
            error: None,
            reference_case: false,
        }
    }

    /// Unit declared by the problem for an output variable (objective,
    /// constraint or unknown), if any.
    fn problem_output_unit(&self, name: &str) -> Option<String> {
        self.problem
            .objectives
            .iter()
            .find(|o| o.variable == name)
            .map(|o| o.unit.clone())
            .or_else(|| {
                self.problem
                    .constraints
                    .iter()
                    .find(|c| c.variable == name)
                    .map(|c| c.unit.clone())
            })
            .or_else(|| {
                self.problem
                    .unknowns
                    .iter()
                    .find(|u| u.name == name)
                    .map(|u| u.unit.clone())
            })
    }

    /// Convert a component output (port unit) into the problem's unit.
    fn to_problem_unit(&self, m: &ComponentManifest, name: &str, value: f64) -> f64 {
        match (
            m.output_schema.iter().find(|p| p.name == name),
            self.problem_output_unit(name),
        ) {
            (Some(p), Some(u)) => simorph_units::convert(value, &p.unit, &u).unwrap_or(value),
            _ => value,
        }
    }

    fn criteria(
        &self,
        planned: &PlannedExperiment,
        vars: &IndexMap<String, f64>,
    ) -> Vec<(String, bool)> {
        planned
            .success_criteria
            .iter()
            .map(|c| {
                let ok = match evaluate(&c.expression, vars) {
                    Ok(Value::Bool(b)) => b,
                    Ok(Value::Num(x)) => x != 0.0,
                    Err(_) => false,
                };
                (c.name.clone(), ok)
            })
            .collect()
    }

    fn objective_scalar(&self, outputs: &IndexMap<String, f64>) -> Option<f64> {
        let mut s = 0.0;
        for o in &self.problem.objectives {
            let v = *outputs.get(&o.variable)?;
            s += o.weight
                * match o.direction {
                    Direction::Minimize => v,
                    Direction::Maximize => -v,
                    Direction::Target => (v - o.target.unwrap_or(0.0)).abs(),
                };
        }
        Some(s)
    }

    fn outcome(
        &self,
        planned: &PlannedExperiment,
        runs: Vec<RunSummary>,
        vars: IndexMap<String, f64>,
        gain: f64,
        wall: f64,
    ) -> ExperimentOutcome {
        let criteria = self.criteria(planned, &vars);
        ExperimentOutcome {
            experiment: planned.id,
            criteria,
            criteria_hash: planned.criteria_hash.clone(),
            information_gain: gain.max(0.0),
            cost: runs.len() as f64,
            wall_time_s: wall,
            runs,
        }
    }

    // ------------------------------------------------------------------ purposes

    fn reference_check(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let cases = &m.validation_protocol.reference_cases;
        let checks = checks::check_reference_cases(comp.as_ref(), cases, ctl);
        let failed = checks.iter().filter(|c| !c.passed).count();
        let mut runs = vec![];
        for (case, chk) in cases.iter().zip(&checks) {
            let mut req = TaskRequest::new(m.id);
            if let Some(obj) = case.inputs.as_object() {
                for (k, v) in obj {
                    req.inputs.insert(k.clone(), v.clone());
                }
            }
            match run_component(comp.as_ref(), &req, ctl) {
                Ok(res) => {
                    let inputs = req
                        .inputs
                        .iter()
                        .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                        .collect();
                    let mut summary = self.validate_run(&m, inputs, &res, vec![chk.clone()]);
                    summary.reference_case = true;
                    runs.push(summary);
                }
                Err(e) => runs.push(RunSummary {
                    run_id: req.run_id,
                    inputs: IndexMap::new(),
                    outputs: IndexMap::new(),
                    evidence: EvidenceLevel::UnverifiedHypothesis,
                    validation: None,
                    valid: false,
                    error: Some(e.to_string()),
                    reference_case: true,
                }),
            }
        }
        let mut vars = IndexMap::new();
        vars.insert("reference_cases_failed".into(), failed as f64);
        vars.insert("reference_cases_total".into(), cases.len() as f64);
        let gain = if cases.is_empty() {
            0.0
        } else if failed == 0 {
            1.0
        } else {
            0.0
        };
        if cases.is_empty() {
            // No declared reference: the criterion "reference_cases_failed == 0" is
            // vacuous, so the runtime refuses to count it as success.
            vars.insert("reference_cases_failed".into(), 1.0);
        }
        Ok(self.outcome(planned, runs, vars, gain, t0.elapsed().as_secs_f64()))
    }

    fn screening(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let factors = self.factors(&m, planned)?;
        let design = Design::latin_hypercube(factors, planned.n_points.max(2), self.seed)?;
        let base = self.base_request(&m, planned)?;
        let mut plan = ExperimentPlan::new(&planned.name, m.id, design, self.seed);
        plan.id = planned.id;
        plan.problem_id = Some(self.problem.id);
        plan.hypothesis_ids = planned.hypotheses.clone();
        plan.fidelity = planned.fidelity;
        for (k, v) in &base.inputs {
            plan.fixed_inputs.insert(k.clone(), v.clone());
        }
        let record = run_experiment(&plan, comp.as_ref(), ctl)?;
        let _ = self.ledger.record_experiment(&record, &[]);
        if let Some(a) = &self.archive {
            a.store(&record)?;
        }
        let mut runs = vec![];
        let mut best: Option<(f64, IndexMap<String, f64>)> = None;
        let mut objectives = vec![];
        for r in &record.runs {
            let inputs: IndexMap<String, f64> = r
                .inputs
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect();
            match &r.result {
                Some(res) => {
                    let s = self.validate_run(&m, inputs.clone(), res, vec![]);
                    if s.valid {
                        if let Some(f) = self.objective_scalar(&s.outputs) {
                            objectives.push(f);
                            if best.as_ref().map(|b| f < b.0).unwrap_or(true) {
                                best = Some((f, inputs.clone()));
                            }
                        }
                    }
                    runs.push(s);
                }
                None => runs.push(RunSummary {
                    run_id: r.run_id,
                    inputs,
                    outputs: IndexMap::new(),
                    evidence: EvidenceLevel::UnverifiedHypothesis,
                    validation: None,
                    valid: false,
                    error: Some(format!("{:?}", r.status)),
                    reference_case: false,
                }),
            }
        }
        // Local sensitivities at the centre of the design (which variables matter).
        let names: Vec<String> = planned
            .design_variables
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let centre: Vec<f64> = record
            .plan
            .design
            .factors
            .iter()
            .map(|f| f.nominal_value())
            .collect();
        let base2 = base.clone();
        let comp2 = comp.clone();
        let spec = self.problem.clone();
        let sens = simorph_uncertainty::local_sensitivity(&names, &centre, 1e-3, 0, |x| {
            let mut req = base2.clone();
            req.run_id = RunId::new();
            for (n, v) in names.iter().zip(x) {
                req.inputs.insert(n.clone(), serde_json::Value::from(*v));
            }
            let res = run_component(comp2.as_ref(), &req, ctl)?;
            let outputs: IndexMap<String, f64> = res
                .outputs
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect();
            let mut s = 0.0;
            for o in &spec.objectives {
                let v = *outputs
                    .get(&o.variable)
                    .ok_or_else(|| SimorphError::NotFound(o.variable.clone()))?;
                s += o.weight
                    * match o.direction {
                        Direction::Minimize => v,
                        Direction::Maximize => -v,
                        Direction::Target => (v - o.target.unwrap_or(0.0)).abs(),
                    };
            }
            Ok(vec![s])
        });
        if let Ok(s) = &sens {
            let _ = self.ledger.record(
                ProvenanceKind::Knowledge,
                format!("sensibilités locales — {}", planned.name),
                s,
                &[],
            );
        }
        let mut vars = IndexMap::new();
        vars.insert("failed_runs".into(), record.n_failed() as f64);
        vars.insert("total_runs".into(), record.runs.len() as f64);
        // information gain: spread of the objective over the design (coefficient of variation)
        let gain = if objectives.len() > 1 {
            let mean = objectives.iter().sum::<f64>() / objectives.len() as f64;
            let var = objectives.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / (objectives.len() - 1) as f64;
            (1.0 + var.sqrt() / mean.abs().max(1e-12)).ln()
        } else {
            0.0
        };
        if let Some(b) = best {
            vars.insert("best_screening_objective".into(), b.0);
            self.best_screening.insert(m.id, b);
        }
        Ok(self.outcome(planned, runs, vars, gain, t0.elapsed().as_secs_f64()))
    }

    fn optimisation(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let base = self.base_request(&m, planned)?;
        // Problem in port units: convert design variable bounds.
        let mut spec = self.problem.clone();
        for d in &mut spec.design_variables {
            let (lo, hi) = (
                self.to_port_unit(&m, &d.name, d.low)?,
                self.to_port_unit(&m, &d.name, d.high)?,
            );
            d.low = lo;
            d.high = hi;
        }
        let problem = ComponentProblem::new(comp.as_ref(), &spec, base, ctl.clone())?;
        let cost = if m.resource_requirements.wall_time_s > 10.0 {
            EvaluationCost::Expensive
        } else if m.resource_requirements.wall_time_s > 0.1 {
            EvaluationCost::Moderate
        } else {
            EvaluationCost::Cheap
        };
        let mut features = problem.features(cost);
        features.budget = planned.n_points.max(10);
        let recs = MethodSelector::recommend(&features);
        let rec = recs
            .first()
            .ok_or_else(|| SimorphError::Other("no optimisation method recommended".into()))?;
        let budget = Budget::new(planned.n_points.max(10)).seed(self.seed);
        let x0: Vec<f64> = self
            .best_screening
            .get(&m.id)
            .map(|(_, inputs)| {
                spec.design_variables
                    .iter()
                    .map(|d| {
                        inputs
                            .get(&d.name)
                            .copied()
                            .unwrap_or(0.5 * (d.low + d.high))
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                spec.design_variables
                    .iter()
                    .map(|d| d.initial.unwrap_or(0.5 * (d.low + d.high)))
                    .collect()
            });
        let result = match rec.method {
            Method::NelderMead => nelder_mead::nelder_mead(&problem, Some(&x0), &budget, ctl)?,
            Method::GradientDescent => {
                gradient::gradient_descent(&problem, Some(&x0), &budget, ctl)?
            }
            Method::Lbfgs => gradient::lbfgs(&problem, Some(&x0), 10, &budget, ctl)?,
            Method::DifferentialEvolution => evolutionary::differential_evolution(
                &problem,
                &evolutionary::DeOptions::default(),
                &budget,
                ctl,
            )?,
            Method::CmaEs => evolutionary::cma_es(
                &problem,
                Some(&x0),
                &evolutionary::CmaOptions::default(),
                &budget,
                ctl,
            )?,
            Method::SimulatedAnnealing => annealing::simulated_annealing(
                &problem,
                Some(&x0),
                &annealing::SaOptions::default(),
                &budget,
                ctl,
            )?,
            Method::BayesianOptimization => bayesian::bayesian_optimization(
                &problem,
                &bayesian::BoOptions::default(),
                &budget,
                ctl,
            )?,
            other => {
                // Multi-objective / discrete / RL methods are not driven by the
                // scalar campaign loop yet: fall back to the best scalar method.
                tracing::warn!("method {other:?} recommended; campaign runtime uses Nelder–Mead for scalarised objectives");
                nelder_mead::nelder_mead(&problem, Some(&x0), &budget, ctl)?
            }
        };
        self.chosen_methods
            .insert(m.id, (rec.method, rec.justification.clone()));
        let _ = self.ledger.record(
            ProvenanceKind::Decision,
            format!("méthode d'optimisation {:?} pour {}", rec.method, m.name),
            &recs,
            &[],
        );
        let log = problem.log.into_inner().unwrap_or_default();
        let mut runs = vec![];
        let mut best_run: Option<RunSummary> = None;
        for (x, res) in &log {
            let inputs: IndexMap<String, f64> = spec
                .design_variables
                .iter()
                .zip(x)
                .map(|(d, v)| (d.name.clone(), *v))
                .collect();
            let s = self.validate_run(&m, inputs, res, vec![]);
            if s.valid
                && x.iter()
                    .zip(&result.best_x)
                    .all(|(a, b)| (a - b).abs() <= 1e-12 * a.abs().max(1.0))
            {
                best_run = Some(s.clone());
            }
            runs.push(s);
        }
        let mut vars = IndexMap::new();
        // Compare in problem units: the best validated run of this optimisation
        // against the best screening run (both scalarised by `objective_scalar`).
        let best_here = runs
            .iter()
            .filter(|r| r.valid)
            .filter_map(|r| self.objective_scalar(&r.outputs))
            .fold(f64::INFINITY, f64::min);
        let improved = self
            .best_screening
            .get(&m.id)
            .map(|(f0, _)| best_here < *f0)
            .unwrap_or(true);
        vars.insert(
            "best_objective_improved".into(),
            if improved { 1.0 } else { 0.0 },
        );
        vars.insert("best_objective".into(), best_here);
        vars.insert("evaluations".into(), result.evaluations as f64);
        vars.insert("converged".into(), if result.converged { 1.0 } else { 0.0 });
        if let Some(b) = &best_run {
            for (k, v) in &b.outputs {
                vars.insert(k.clone(), *v);
            }
            self.best_optimised
                .insert(m.id, (result.best_x.clone(), b.outputs.clone()));
        }
        let gain = self
            .best_screening
            .get(&m.id)
            .map(|(f0, _)| ((f0 - best_here) / f0.abs().max(1e-12)).max(0.0))
            .unwrap_or(1.0);
        Ok(self.outcome(planned, runs, vars, gain, t0.elapsed().as_secs_f64()))
    }

    fn verification(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let (x, low_outputs) = self
            .best_optimised
            .get(&m.id)
            .cloned()
            .ok_or_else(|| SimorphError::InvalidInput("no optimised point to verify".into()))?;
        let mut req = self.base_request(&m, planned)?.fidelity(1.0);
        for (d, v) in self.problem.design_variables.iter().zip(&x) {
            req.inputs
                .insert(d.name.clone(), serde_json::Value::from(*v));
        }
        let res = run_component(comp.as_ref(), &req, ctl)?;
        let again = run_component(comp.as_ref(), &req, ctl)?;
        let repro = checks::check_reproducibility(&res, &again, 1e-9, 1e-12);
        let inputs: IndexMap<String, f64> = req
            .inputs
            .iter()
            .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
            .collect();
        let s = self.validate_run(&m, inputs, &res, vec![repro]);
        let mut rel = 0.0f64;
        for o in &self.problem.objectives {
            if let (Some(a), Some(b)) = (s.outputs.get(&o.variable), low_outputs.get(&o.variable)) {
                rel = rel.max((a - b).abs() / b.abs().max(1e-12));
            }
        }
        let mut vars = IndexMap::new();
        vars.insert("relative_change_vs_low_fidelity".into(), rel);
        for (k, v) in &s.outputs {
            vars.insert(k.clone(), *v);
        }
        let gain = (1.0 - rel).max(0.0);
        Ok(self.outcome(planned, vec![s], vars, gain, t0.elapsed().as_secs_f64()))
    }

    fn calibration(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        if self.observations.is_empty() {
            return Err(SimorphError::InvalidInput(
                "calibration planned but no observations supplied".into(),
            ));
        }
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let (mut pred, mut refv, mut refs) = (vec![], vec![], vec![]);
        let mut runs = vec![];
        for obs in &self.observations.clone() {
            let mut req = self.base_request(&m, planned)?;
            for (k, v) in &obs.inputs {
                req.inputs.insert(
                    k.clone(),
                    serde_json::Value::from(self.to_port_unit(&m, k, *v)?),
                );
            }
            let res = run_component(comp.as_ref(), &req, ctl)?;
            let s = self.validate_run(&m, obs.inputs.clone(), &res, vec![]);
            if let Some(p) = s.outputs.get(&obs.output) {
                pred.push(*p);
                refv.push(obs.value);
                refs.push(obs.std);
            }
            runs.push(s);
        }
        let z = checks::check_z_test(&pred, &refv, &refs, 0.05);
        let zscore = z
            .metrics
            .get("z")
            .or_else(|| z.metrics.values().next())
            .copied()
            .unwrap_or(f64::INFINITY);
        let _ = self.ledger.record(
            ProvenanceKind::Validation,
            format!("comparaison aux observations — {}", m.name),
            &z,
            &[],
        );
        let mut vars = IndexMap::new();
        vars.insert("z_score".into(), zscore.abs());
        vars.insert("observations".into(), pred.len() as f64);
        let gain = if z.passed { 2.0 } else { 0.5 };
        Ok(self.outcome(planned, runs, vars, gain, t0.elapsed().as_secs_f64()))
    }

    fn counter_example(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let comp = self.component(planned.component)?;
        let m = comp.manifest().clone();
        let t0 = std::time::Instant::now();
        let factors = self.factors(&m, planned)?;
        let base = self.base_request(&m, planned)?;
        let constraints = self.problem.constraints.clone();
        let ce = find_counter_example(
            comp.as_ref(),
            factors,
            &base.inputs,
            |res| {
                constraints
                    .iter()
                    .filter(|c| c.hard)
                    .all(|c| match res.get_f64(&c.variable) {
                        Ok(v) => match c.kind {
                            ConstraintKind::UpperBound => v <= c.bound + c.tolerance,
                            ConstraintKind::LowerBound => v >= c.bound - c.tolerance,
                            ConstraintKind::Equality => {
                                (v - c.bound).abs() <= c.tolerance.max(1e-12)
                            }
                        },
                        Err(_) => false,
                    })
            },
            planned.n_points.max(10),
            8,
            self.seed,
            ctl,
        )?;
        let mut vars = IndexMap::new();
        let mut runs = vec![];
        match ce {
            Some(c) => {
                let inputs = c
                    .inputs
                    .iter()
                    .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                    .collect();
                runs.push(self.validate_run(&m, inputs, &c.result, vec![]));
                vars.insert("counter_example_found".into(), 1.0);
                vars.insert("evaluations".into(), c.evaluations as f64);
            }
            None => {
                vars.insert("counter_example_found".into(), 0.0);
            }
        }
        let gain = 1.0;
        Ok(self.outcome(planned, runs, vars, gain, t0.elapsed().as_secs_f64()))
    }
}

impl ExperimentExecutor for CampaignRuntime {
    fn run(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome> {
        let _ = self.ledger.record(
            ProvenanceKind::Experiment,
            format!("début : {}", planned.name),
            planned,
            &[],
        );
        let out = match planned.purpose {
            ExperimentPurpose::ReferenceCheck => self.reference_check(planned, ctl),
            ExperimentPurpose::Screening | ExperimentPurpose::Sensitivity => {
                self.screening(planned, ctl)
            }
            ExperimentPurpose::Optimisation => self.optimisation(planned, ctl),
            ExperimentPurpose::Verification => self.verification(planned, ctl),
            ExperimentPurpose::Calibration => self.calibration(planned, ctl),
            ExperimentPurpose::CounterExample => self.counter_example(planned, ctl),
        }?;
        let _ = self.ledger.record(
            ProvenanceKind::Decision,
            format!(
                "fin : {} ({})",
                planned.name,
                if out.succeeded() {
                    "critères satisfaits"
                } else {
                    "critères non satisfaits"
                }
            ),
            &out.criteria,
            &[],
        );
        Ok(out)
    }
}

/// Convenience: run a complete campaign for a problem with the given components.
pub fn run_campaign(
    problem: ProblemSpec,
    components: ComponentSet,
    ledger: Ledger,
    ctl: &ExecutionControl,
) -> Result<simorph_svc_scientific_director::Campaign> {
    use simorph_svc_scientific_director as dir;
    let catalog = dir::StaticCatalog::new(components.manifests());
    let analysis = dir::analyse(&problem, &catalog)?;
    let hypotheses = dir::formulate(&problem, &analysis);
    let programme = dir::plan(&problem, &analysis, &hypotheses);
    let _ = ledger.record(
        ProvenanceKind::Problem,
        problem.title.clone(),
        &problem,
        &[],
    );
    let _ = ledger.record(
        ProvenanceKind::Decision,
        "analyse du problème",
        &analysis,
        &[],
    );
    let mut campaign = dir::Campaign::new(problem.clone(), analysis, hypotheses, programme);
    let mut rt = CampaignRuntime::new(problem, components, ledger);
    campaign.execute(&mut rt, ctl)?;
    Ok(campaign)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_plugin_engineering as eng;
    use std::sync::Arc;

    #[test]
    fn beam_campaign_runs_end_to_end_with_unit_conversion() {
        let set = ComponentSet::new();
        set.register(|| Ok(Arc::new(eng::beam::Cantilever::new()) as Arc<dyn Component>))
            .unwrap();
        let mut p = ProblemSpec::new(
            "Poutre console",
            "Minimiser la flèche d'une poutre console sous charge",
        );
        p.domains.push("civil-engineering".into());
        p.knowns.push(Quantity::new("load", 1.0, "kN")); // kN → N conversion
        p.knowns.push(Quantity::new("youngs_modulus", 210.0, "GPa"));
        p.design_variables.push(DesignVariable {
            name: "length".into(),
            unit: "mm".into(),
            low: 1000.0,
            high: 3000.0,
            integer: false,
            initial: None,
        });
        p.design_variables.push(DesignVariable {
            name: "second_moment".into(),
            unit: "cm^4".into(),
            low: 500.0,
            high: 2000.0,
            integer: false,
            initial: None,
        });
        p.objectives.push(Objective {
            name: "flèche minimale".into(),
            direction: Direction::Minimize,
            variable: "tip_deflection".into(),
            unit: "m".into(),
            target: None,
            weight: 1.0,
        });
        p.constraints.push(Constraint {
            name: "flèche admissible".into(),
            variable: "tip_deflection".into(),
            kind: ConstraintKind::UpperBound,
            bound: 0.02,
            unit: "m".into(),
            tolerance: 0.0,
            hard: true,
        });
        p.budget.max_runs = 2000;
        let ledger = Ledger::in_memory();
        let c = run_campaign(p, set, ledger.clone(), &ExecutionControl::default()).unwrap();
        assert_eq!(
            c.stop_reason,
            Some(simorph_svc_scientific_director::StopReason::ProgrammeCompleted),
            "{}",
            c.report_markdown()
        );
        assert!(
            c.outcomes.iter().all(|o| o.succeeded()),
            "{}",
            c.report_markdown()
        );
        // best: shortest beam, largest inertia: δ = P L³/(3 E I) = 1000 * 1 / (3 * 2.1e11 * 2e-5)
        let expected = 1000.0 / (3.0 * 2.1e11 * 2e-5);
        let concl = c
            .conclusions
            .iter()
            .find(|k| k.variable == "tip_deflection")
            .unwrap();
        assert!(
            (concl.value - expected).abs() / expected < 0.02,
            "got {} expected {expected}",
            concl.value
        );
        assert_eq!(concl.level, EvidenceLevel::Computed);
        assert!(!concl.validations.is_empty());
        ledger.verify_chain().unwrap();
        assert!(ledger.by_kind(ProvenanceKind::Validation).len() > 10);
    }
}
