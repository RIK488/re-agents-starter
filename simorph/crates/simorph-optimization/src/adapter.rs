//! Adapter: optimise any `simorph_core::Component` against a `ProblemSpec`.
//!
//! Design variables become the decision vector, objectives are read from
//! the component outputs (maximisation negated, targets turned into
//! |y − target|), and constraints become `g(x) <= 0` values.
//!
//! Units (audit): design variables are expressed in the spec's units and are
//! converted to the component's input-port units; outputs are converted from
//! the output-port unit to the unit of the objective / constraint before
//! targets, bounds and tolerances are applied.  Equal units are left
//! untouched, so the conversion is idempotent; incompatible dimensions are
//! refused when the problem is built.
//!
//! Weights and soft constraints (audit): each objective is multiplied by its
//! `weight` (negative or non-finite weights are refused); only `hard`
//! constraints become `g(x) <= 0` constraints — soft ones add
//! `soft_penalty · max(0, g)²` to every objective instead of being treated
//! as hard.

use crate::problem::{Evaluation, ObjectiveFunction};
use crate::selector::{EvaluationCost, ProblemFeatures, VariableKind};
use simorph_core::{
    Component, ConstraintKind, Direction, ExecutionControl, ProblemSpec, Result, SimorphError,
    TaskRequest, TaskResult,
};
use std::sync::Mutex;

/// Evaluates a component for each decision vector.
pub struct ComponentProblem<'a> {
    pub component: &'a dyn Component,
    pub spec: &'a ProblemSpec,
    /// Base request: fixed inputs (knowns) that are not design variables.
    pub base_request: TaskRequest,
    pub control: ExecutionControl,
    /// Every evaluation's (x, TaskResult) is recorded for provenance.
    pub log: Mutex<Vec<(Vec<f64>, TaskResult)>>,
    /// Penalty factor applied to soft-constraint violations (default 1e3).
    pub soft_penalty: f64,
    /// Per design variable: (spec unit, input-port unit) when they differ.
    input_conv: Vec<Option<(simorph_units::Unit, simorph_units::Unit)>>,
    /// Per objective: (output-port unit, objective unit) when they differ.
    objective_conv: Vec<Option<(simorph_units::Unit, simorph_units::Unit)>>,
    /// Per constraint: (output-port unit, constraint unit) when they differ.
    constraint_conv: Vec<Option<(simorph_units::Unit, simorph_units::Unit)>>,
}

/// Conversion between two unit expressions, `None` when they are textually
/// equal or one side is undeclared.  Incompatible dimensions (or log units)
/// are an error.
fn conversion(
    from: Option<&str>,
    to: &str,
    what: &str,
) -> Result<Option<(simorph_units::Unit, simorph_units::Unit)>> {
    let Some(from) = from else { return Ok(None) };
    let norm = |u: &str| {
        let t = u.trim();
        if t.is_empty() || t == "-" {
            "1".to_string()
        } else {
            t.to_string()
        }
    };
    if norm(from) == norm(to) || to.trim().is_empty() {
        return Ok(None);
    }
    // validates dimensions and refuses logarithmic units
    simorph_units::convert(1.0, from, to).map_err(|e| {
        SimorphError::InvalidInput(format!("{what}: cannot convert `{from}` to `{to}`: {e}"))
    })?;
    Ok(Some((
        simorph_units::parse_unit(from)?,
        simorph_units::parse_unit(to)?,
    )))
}

fn apply(conv: &Option<(simorph_units::Unit, simorph_units::Unit)>, v: f64) -> f64 {
    match conv {
        Some((from, to)) => to.from_si(from.to_si(v)),
        None => v,
    }
}

impl<'a> ComponentProblem<'a> {
    pub fn new(
        component: &'a dyn Component,
        spec: &'a ProblemSpec,
        base_request: TaskRequest,
        control: ExecutionControl,
    ) -> Result<Self> {
        if spec.design_variables.is_empty() {
            return Err(SimorphError::InvalidInput(
                "problem has no design variables".into(),
            ));
        }
        if spec.objectives.is_empty() {
            return Err(SimorphError::InvalidInput(
                "problem has no objectives".into(),
            ));
        }
        for o in &spec.objectives {
            if !(o.weight.is_finite() && o.weight >= 0.0) {
                return Err(SimorphError::InvalidInput(format!(
                    "objective `{}`: weight must be finite and non-negative (got {})",
                    o.name, o.weight
                )));
            }
        }
        let m = component.manifest();
        let in_unit = |n: &str| {
            m.input_schema
                .iter()
                .find(|p| p.name == n)
                .map(|p| p.unit.as_str())
        };
        let out_unit = |n: &str| {
            m.output_schema
                .iter()
                .find(|p| p.name == n)
                .map(|p| p.unit.as_str())
        };
        let input_conv = spec
            .design_variables
            .iter()
            .map(|d| conversion(Some(&d.unit), in_unit(&d.name).unwrap_or(&d.unit), &d.name))
            .collect::<Result<Vec<_>>>()?;
        let objective_conv = spec
            .objectives
            .iter()
            .map(|o| conversion(out_unit(&o.variable), &o.unit, &o.name))
            .collect::<Result<Vec<_>>>()?;
        let constraint_conv = spec
            .constraints
            .iter()
            .map(|c| conversion(out_unit(&c.variable), &c.unit, &c.name))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            component,
            spec,
            base_request,
            control,
            log: Mutex::new(vec![]),
            soft_penalty: 1e3,
            input_conv,
            objective_conv,
            constraint_conv,
        })
    }

    /// Build the request for a decision vector.
    pub fn request_for(&self, x: &[f64]) -> TaskRequest {
        let mut req = self.base_request.clone();
        req.run_id = simorph_core::RunId::new();
        for (i, (dv, &v)) in self.spec.design_variables.iter().zip(x).enumerate() {
            let v = if dv.integer { v.round() } else { v };
            let v = self.input_conv.get(i).map(|c| apply(c, v)).unwrap_or(v);
            req.inputs
                .insert(dv.name.clone(), serde_json::Value::from(v));
        }
        req
    }

    /// Problem features derived from the spec, for the method selector.
    pub fn features(&self, evaluation_cost: EvaluationCost) -> ProblemFeatures {
        let ints = self
            .spec
            .design_variables
            .iter()
            .filter(|d| d.integer)
            .count();
        ProblemFeatures {
            dimension: self.spec.design_variables.len(),
            variables: if ints == 0 {
                VariableKind::Continuous
            } else if ints == self.spec.design_variables.len() {
                VariableKind::Integer
            } else {
                VariableKind::Mixed
            },
            has_gradient: false,
            smooth: true,
            noisy: !matches!(
                self.component.manifest().deterministic_mode,
                simorph_core::DeterministicMode::Always | simorph_core::DeterministicMode::WithSeed
            ),
            multimodal: false,
            n_objectives: self.spec.objectives.len(),
            n_constraints: self.n_constraints(),
            evaluation_cost,
            budget: self.spec.budget.max_runs as usize,
            uncertain_parameters: self.spec.knowns.iter().any(|q| {
                !matches!(
                    q.uncertainty,
                    simorph_core::Uncertainty::Exact | simorph_core::Uncertainty::Undeclared
                )
            }),
            sequential_decisions: false,
        }
    }
}

impl<'a> ObjectiveFunction for ComponentProblem<'a> {
    fn dimension(&self) -> usize {
        self.spec.design_variables.len()
    }
    fn bounds(&self) -> Vec<(f64, f64)> {
        self.spec
            .design_variables
            .iter()
            .map(|d| (d.low, d.high))
            .collect()
    }
    fn n_objectives(&self) -> usize {
        self.spec.objectives.len()
    }
    fn n_constraints(&self) -> usize {
        self.spec.constraints.iter().filter(|c| c.hard).count()
    }
    fn integer_variables(&self) -> Vec<usize> {
        self.spec
            .design_variables
            .iter()
            .enumerate()
            .filter(|(_, d)| d.integer)
            .map(|(i, _)| i)
            .collect()
    }
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
        self.control.check()?;
        let req = self.request_for(x);
        let res = simorph_core::run_component(self.component, &req, &self.control)?;
        let mut objectives = Vec::with_capacity(self.spec.objectives.len());
        for (i, o) in self.spec.objectives.iter().enumerate() {
            let y = apply(&self.objective_conv[i], res.get_f64(&o.variable)?);
            let v = match o.direction {
                Direction::Minimize => y,
                Direction::Maximize => -y,
                Direction::Target => (y - o.target.unwrap_or(0.0)).abs(),
            };
            objectives.push(o.weight * v);
        }
        let mut constraints = Vec::with_capacity(self.spec.constraints.len());
        let mut soft = 0.0;
        for (i, c) in self.spec.constraints.iter().enumerate() {
            let y = apply(&self.constraint_conv[i], res.get_f64(&c.variable)?);
            let g = match c.kind {
                ConstraintKind::UpperBound => y - c.bound - c.tolerance,
                ConstraintKind::LowerBound => c.bound - c.tolerance - y,
                ConstraintKind::Equality => (y - c.bound).abs() - c.tolerance,
            };
            if c.hard {
                constraints.push(g);
            } else {
                soft += g.max(0.0).powi(2);
            }
        }
        if soft != 0.0 {
            for o in objectives.iter_mut() {
                *o += self.soft_penalty * soft;
            }
        }
        let cost = res.wall_time_ms as f64 / 1000.0;
        self.log.lock().unwrap().push((x.to_vec(), res));
        Ok(Evaluation {
            objectives,
            constraints,
            cost,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::*;

    struct Parabola {
        m: ComponentManifest,
    }
    impl Component for Parabola {
        fn manifest(&self) -> &ComponentManifest {
            &self.m
        }
        fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
            let x = req.get_f64("x")?;
            let k = req.get_f64("k")?;
            Ok(TaskResult::new(req, EvidenceLevel::Computed)
                .output("y", k * (x - 3.0).powi(2))
                .output("z", x))
        }
    }

    #[test]
    fn optimises_component_against_spec() {
        let comp = Parabola {
            m: ComponentManifest::in_process("parabola", "1", "t")
                .with_inputs(vec![Port::scalar("x", "m"), Port::scalar("k", "1")])
                .with_outputs(vec![Port::scalar("y", "m^2"), Port::scalar("z", "m")]),
        };
        let mut spec = ProblemSpec::new("min parabola", "");
        spec.design_variables.push(DesignVariable {
            name: "x".into(),
            unit: "m".into(),
            low: -10.0,
            high: 10.0,
            integer: false,
            initial: None,
        });
        spec.objectives.push(Objective {
            name: "y".into(),
            direction: Direction::Minimize,
            variable: "y".into(),
            unit: "m^2".into(),
            target: None,
            weight: 1.0,
        });
        spec.constraints.push(Constraint {
            name: "z cap".into(),
            variable: "z".into(),
            kind: ConstraintKind::UpperBound,
            bound: 2.0,
            unit: "m".into(),
            tolerance: 0.0,
            hard: true,
        });
        let base = TaskRequest::new(comp.m.id).input("k", 2.0);
        let p = ComponentProblem::new(&comp, &spec, base, ExecutionControl::default()).unwrap();
        let r = crate::evolutionary::differential_evolution(
            &p,
            &Default::default(),
            &crate::problem::Budget::new(3000).tolerance(1e-12, 1e-9),
            &ExecutionControl::default(),
        )
        .unwrap();
        // constrained optimum: x = 2, y = 2
        assert!((r.best_x[0] - 2.0).abs() < 1e-3, "x={}", r.best_x[0]);
        assert!(r.best_eval.is_feasible());
        assert_eq!(p.log.lock().unwrap().len(), r.evaluations);
        let f = p.features(EvaluationCost::Cheap);
        assert_eq!(f.n_constraints, 1);
    }
}
