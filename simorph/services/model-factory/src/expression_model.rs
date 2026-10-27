//! Executable models from user-typed equations.

use crate::dimension::{check_expression_unit, symbols};
use indexmap::IndexMap;
use simorph_core::{Port, Quantity, ReferenceCase, Result, SimorphError};
use simorph_model_registry::{Formalism, ModelDescriptor};
use simorph_simulation::{DynamicalModel, Params};
use simorph_validation::{evaluate, CheckResult};

/// One parsed equation line.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedEquation {
    /// `d(x)/dt = expr` or `dx/dt = expr`
    Derivative { state: String, expr: String },
    /// `y = expr`
    Algebraic { output: String, expr: String },
}

/// Parse one line.  Accepted left-hand sides: `d(x)/dt`, `dx/dt`, `x'`, `y`.
pub fn parse_equation(line: &str) -> Result<ParsedEquation> {
    let (lhs, rhs) = line
        .split_once('=')
        .ok_or_else(|| SimorphError::InvalidInput(format!("equation `{line}` has no `=`")))?;
    let lhs = lhs.trim();
    let expr = rhs.trim().to_string();
    if expr.is_empty() {
        return Err(SimorphError::InvalidInput(format!(
            "equation `{line}` has an empty right-hand side"
        )));
    }
    let valid_ident = |s: &str| {
        !s.is_empty()
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !s.starts_with(|c: char| c.is_ascii_digit())
    };
    if let Some(inner) = lhs.strip_prefix("d(").and_then(|s| s.strip_suffix(")/dt")) {
        if valid_ident(inner.trim()) {
            return Ok(ParsedEquation::Derivative {
                state: inner.trim().to_string(),
                expr,
            });
        }
    }
    if let Some(inner) = lhs.strip_prefix('d').and_then(|s| s.strip_suffix("/dt")) {
        if valid_ident(inner) {
            return Ok(ParsedEquation::Derivative {
                state: inner.to_string(),
                expr,
            });
        }
    }
    if let Some(inner) = lhs.strip_suffix('\'') {
        if valid_ident(inner) {
            return Ok(ParsedEquation::Derivative {
                state: inner.to_string(),
                expr,
            });
        }
    }
    if valid_ident(lhs) {
        return Ok(ParsedEquation::Algebraic {
            output: lhs.to_string(),
            expr,
        });
    }
    Err(SimorphError::InvalidInput(format!(
        "cannot parse left-hand side `{lhs}` (use `d(x)/dt = …` or `y = …`)"
    )))
}

/// A dynamical model defined by equations, evaluated with the safe
/// expression evaluator.  States, outputs and parameters carry units and
/// every equation is dimensionally checked at construction.
#[derive(Debug, Clone)]
pub struct ExpressionModel {
    name: String,
    states: Vec<(String, String)>,
    derivatives: Vec<String>,
    /// Algebraic outputs in declaration order (may depend on earlier ones).
    algebraic: Vec<(String, String, String)>,
    parameters: Vec<(String, String)>,
    defaults: IndexMap<String, f64>,
    assumptions: Vec<String>,
    reference_cases: Vec<ReferenceCase>,
    domains: Vec<String>,
    /// Dimensional check results (one per equation).
    pub dimensional_report: Vec<CheckResult>,
}

impl ExpressionModel {
    /// Build from equation lines.  `variables` declare units of states and
    /// algebraic outputs; `parameters` declare the parameters with default
    /// values and units.  Initial conditions are parameters named
    /// `<state>_0` (added automatically with the state's unit).
    pub fn new(
        name: &str,
        equations: &[&str],
        variables: &[Port],
        parameters: &[Quantity],
    ) -> Result<Self> {
        if equations.is_empty() {
            return Err(SimorphError::InvalidInput("no equations".into()));
        }
        let parsed: Vec<ParsedEquation> = equations
            .iter()
            .map(|e| parse_equation(e))
            .collect::<Result<_>>()?;
        let unit_of = |n: &str| {
            variables
                .iter()
                .find(|p| p.name == n)
                .map(|p| p.unit.clone())
        };
        let mut states = vec![];
        let mut derivatives = vec![];
        let mut algebraic = vec![];
        for p in &parsed {
            match p {
                ParsedEquation::Derivative { state, expr } => {
                    let u = unit_of(state).ok_or_else(|| {
                        SimorphError::InvalidInput(format!(
                            "state `{state}` has no declared unit (add it to `variables`)"
                        ))
                    })?;
                    if states.iter().any(|(s, _)| s == state) {
                        return Err(SimorphError::InvalidInput(format!(
                            "state `{state}` defined twice"
                        )));
                    }
                    states.push((state.clone(), u));
                    derivatives.push(expr.clone());
                }
                ParsedEquation::Algebraic { output, expr } => {
                    let u = unit_of(output).ok_or_else(|| {
                        SimorphError::InvalidInput(format!(
                            "output `{output}` has no declared unit (add it to `variables`)"
                        ))
                    })?;
                    algebraic.push((output.clone(), u, expr.clone()));
                }
            }
        }
        let mut params: Vec<(String, String)> = parameters
            .iter()
            .map(|q| (q.name.clone(), q.unit.clone()))
            .collect();
        let mut defaults: IndexMap<String, f64> = parameters
            .iter()
            .map(|q| (q.name.clone(), q.value))
            .collect();
        for (s, u) in &states {
            let ic = format!("{s}_0");
            if !params.iter().any(|(n, _)| n == &ic) {
                params.push((ic.clone(), u.clone()));
                defaults.insert(ic, 0.0);
            }
        }
        // symbol table for dimensional analysis and unknown-symbol detection
        let mut units: IndexMap<String, String> = IndexMap::new();
        units.insert("t".into(), "s".into());
        for (n, u) in states.iter().chain(params.iter()) {
            units.insert(n.clone(), u.clone());
        }
        let mut report = vec![];
        for (i, p) in parsed.iter().enumerate() {
            let (lhs, expected, expr) = match p {
                ParsedEquation::Derivative { state, expr } => (
                    format!("d({state})/dt"),
                    format!("({})/s", unit_of(state).unwrap()),
                    expr,
                ),
                ParsedEquation::Algebraic { output, expr } => {
                    (output.clone(), unit_of(output).unwrap(), expr)
                }
            };
            let syms = symbols(expr)?;
            if let Some(unknown) = syms.iter().find(|s| !units.contains_key(*s)) {
                return Err(SimorphError::InvalidInput(format!(
                    "equation {} uses undefined symbol `{unknown}`",
                    i + 1
                )));
            }
            match check_expression_unit(expr, &expected, &units) {
                Ok(()) => report.push(CheckResult::pass(
                    "equation_dimensions",
                    format!("{lhs} = {expr} ~ [{expected}]"),
                )),
                Err(e) => {
                    return Err(SimorphError::Dimensional(format!(
                        "equation `{lhs} = {expr}`: {e}"
                    )))
                }
            }
            if let ParsedEquation::Algebraic { output, .. } = p {
                units.insert(output.clone(), unit_of(output).unwrap());
            }
        }
        Ok(Self {
            name: name.to_string(),
            states,
            derivatives,
            algebraic,
            parameters: params,
            defaults,
            assumptions: vec![
                "model built from user-supplied explicit first-order equations; no physical validation beyond dimensional consistency".into(),
            ],
            reference_cases: vec![],
            domains: vec![],
            dimensional_report: report,
        })
    }

    pub fn with_reference_case(mut self, c: ReferenceCase) -> Self {
        self.reference_cases.push(c);
        self
    }
    pub fn with_domains(mut self, d: &[&str]) -> Self {
        self.domains = d.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn with_assumption(mut self, a: &str) -> Self {
        self.assumptions.push(a.to_string());
        self
    }
    pub fn default_params(&self) -> Params {
        self.defaults.clone()
    }
    pub fn equations(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .states
            .iter()
            .zip(&self.derivatives)
            .map(|((s, _), e)| (format!("d({s})/dt"), e.clone()))
            .collect();
        v.extend(
            self.algebraic
                .iter()
                .map(|(o, _, e)| (o.clone(), e.clone())),
        );
        v
    }

    fn vars(&self, t: f64, x: &[f64], p: &Params) -> IndexMap<String, f64> {
        let mut vars: IndexMap<String, f64> = p.clone();
        for (k, v) in &self.defaults {
            vars.entry(k.clone()).or_insert(*v);
        }
        vars.insert("t".into(), t);
        for ((s, _), v) in self.states.iter().zip(x) {
            vars.insert(s.clone(), *v);
        }
        vars
    }

    fn eval(expr: &str, vars: &IndexMap<String, f64>) -> Result<f64> {
        evaluate(expr, vars)?.as_f64()
    }

    /// Descriptor of this model (REQ-MOD-2).
    pub fn descriptor(&self, version: &str) -> ModelDescriptor {
        let formalism = if self.states.is_empty() {
            Formalism::Algebraic
        } else {
            Formalism::Ode
        };
        let mut d = ModelDescriptor::new(&self.name, version, formalism)
            .with_states(
                self.states
                    .iter()
                    .map(|(n, u)| Port::scalar(n, u))
                    .collect(),
            )
            .with_outputs(
                self.algebraic
                    .iter()
                    .map(|(n, u, _)| Port::scalar(n, u))
                    .collect(),
            )
            .with_inputs(
                self.parameters
                    .iter()
                    .map(|(n, u)| Port::scalar(n, u))
                    .collect(),
            )
            .with_parameters(
                self.parameters
                    .iter()
                    .map(|(n, u)| Quantity::new(n, self.defaults.get(n).copied().unwrap_or(0.0), u))
                    .collect(),
            )
            .with_domains(&self.domains.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        for (lhs, rhs) in self.equations() {
            d = d.with_equation(&lhs, &rhs);
        }
        d.assumptions = self.assumptions.clone();
        for c in &self.reference_cases {
            d = d.with_criterion(
                &format!("reference:{}", c.name),
                &format!(
                    "matches reference case `{}` within rtol={} atol={}",
                    c.name, c.rtol, c.atol
                ),
            );
        }
        d
    }
}

impl DynamicalModel for ExpressionModel {
    fn name(&self) -> &str {
        &self.name
    }
    fn state_dim(&self) -> usize {
        self.states.len()
    }
    fn state_names(&self) -> Vec<(String, String)> {
        self.states.clone()
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        self.parameters.clone()
    }
    fn output_names(&self) -> Vec<(String, String)> {
        self.algebraic
            .iter()
            .map(|(n, u, _)| (n.clone(), u.clone()))
            .collect()
    }
    fn initial_state(&self, params: &Params) -> Vec<f64> {
        self.states
            .iter()
            .map(|(s, _)| {
                let ic = format!("{s}_0");
                params
                    .get(&ic)
                    .or_else(|| self.defaults.get(&ic))
                    .copied()
                    .unwrap_or(0.0)
            })
            .collect()
    }
    fn rhs(&self, t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> Result<()> {
        let mut vars = self.vars(t, x, params);
        for (o, _, e) in &self.algebraic {
            let v = Self::eval(e, &vars)?;
            vars.insert(o.clone(), v);
        }
        for (i, e) in self.derivatives.iter().enumerate() {
            dx[i] = Self::eval(e, &vars)?;
        }
        Ok(())
    }
    fn outputs(&self, t: f64, x: &[f64], params: &Params) -> IndexMap<String, f64> {
        let mut vars = self.vars(t, x, params);
        let mut out = IndexMap::new();
        for (o, _, e) in &self.algebraic {
            if let Ok(v) = Self::eval(e, &vars) {
                vars.insert(o.clone(), v);
                out.insert(o.clone(), v);
            }
        }
        out
    }
    fn assumptions(&self) -> Vec<String> {
        self.assumptions.clone()
    }
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        self.reference_cases.clone()
    }
    fn domains(&self) -> Vec<String> {
        self.domains.clone()
    }
}

/// Convenience used by the factory API: build the descriptor straight from equations.
pub fn from_equations(
    name: &str,
    equations: &[&str],
    variables: &[Port],
    parameters: &[Quantity],
) -> Result<ModelDescriptor> {
    Ok(ExpressionModel::new(name, equations, variables, parameters)?.descriptor("0.1.0"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::ExecutionControl;
    use simorph_simulation::{simulate, Solver, SolverOptions};

    #[test]
    fn decay_matches_analytic() {
        let m = ExpressionModel::new(
            "decay",
            &["d(x)/dt = -k*x"],
            &[Port::scalar("x", "mol")],
            &[Quantity::new("k", 0.5, "1/s")],
        )
        .unwrap();
        let mut p = m.default_params();
        p.insert("x_0".into(), 2.0);
        let traj = simulate(
            &m,
            &p,
            None,
            (0.0, 4.0),
            &SolverOptions::new(Solver::Rk45).with_tolerances(1e-9, 1e-12),
            &ExecutionControl::default(),
        )
        .unwrap();
        let exact = 2.0 * (-0.5f64 * 4.0).exp();
        assert!((traj.final_state()[0] - exact).abs() < 1e-7);
        assert_eq!(m.dimensional_report.len(), 1);
        let d = m.descriptor("1");
        assert_eq!(d.formalism, Formalism::Ode);
        assert!(d.inputs.iter().any(|p| p.name == "x_0" && p.unit == "mol"));
    }

    #[test]
    fn oscillator_with_algebraic_output() {
        let m = ExpressionModel::new(
            "oscillator",
            &[
                "d(x)/dt = v",
                "d(v)/dt = -(k/m)*x",
                "energy = 0.5*m*v^2 + 0.5*k*x^2",
            ],
            &[
                Port::scalar("x", "m"),
                Port::scalar("v", "m/s"),
                Port::scalar("energy", "J"),
            ],
            &[
                Quantity::new("k", 4.0, "N/m"),
                Quantity::new("m", 1.0, "kg"),
            ],
        )
        .unwrap();
        let mut p = m.default_params();
        p.insert("x_0".into(), 1.0);
        let traj = simulate(
            &m,
            &p,
            None,
            (0.0, std::f64::consts::PI),
            &SolverOptions::new(Solver::Rk45).with_tolerances(1e-10, 1e-12),
            &ExecutionControl::default(),
        )
        .unwrap();
        // omega = 2 rad/s → one full period at t = pi
        assert!((traj.final_state()[0] - 1.0).abs() < 1e-6);
        let e = &traj.outputs["energy"];
        assert!((e[0] - 2.0).abs() < 1e-12 && (e.last().unwrap() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn inconsistent_and_undefined_rejected() {
        let r = ExpressionModel::new(
            "bad",
            &["d(x)/dt = -k*x + t"],
            &[Port::scalar("x", "mol")],
            &[Quantity::new("k", 0.5, "1/s")],
        );
        assert!(matches!(r, Err(SimorphError::Dimensional(_))), "{r:?}");
        let r = ExpressionModel::new(
            "bad",
            &["d(x)/dt = -q*x"],
            &[Port::scalar("x", "mol")],
            &[Quantity::new("k", 0.5, "1/s")],
        );
        assert!(matches!(r, Err(SimorphError::InvalidInput(_))));
        assert!(parse_equation("x' = 1").is_ok());
        assert!(parse_equation("2x = 1").is_err());
    }
}
