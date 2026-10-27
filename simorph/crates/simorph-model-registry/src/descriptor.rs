//! Declarative model descriptor.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{ComponentId, ModelId, Port, Quantity, UncertaintyModel, ValidityDomain};

/// Mathematical formalism of a model (§3.3 list).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Formalism {
    /// Ordinary differential equations.
    Ode,
    /// Differential-algebraic equations.
    Dae,
    /// Partial differential equations.
    Pde,
    /// Algebraic system.
    Algebraic,
    /// Logical / constraint model.
    Logical,
    /// Probabilistic / Bayesian model.
    Probabilistic,
    /// Dynamical system in the abstract sense (map / flow).
    DynamicalSystem,
    /// Graph / network.
    Graph,
    /// Geometry / CAD.
    Geometric,
    /// Multi-agent system.
    MultiAgent,
    /// Discrete-event process.
    DiscreteEvent,
    /// Continuous-time model.
    Continuous,
    /// Hybrid discrete/continuous.
    Hybrid,
    /// Data-driven (learned) model.
    DataDriven,
    /// Surrogate / reduced-order model of another model.
    Surrogate,
    /// Physics-based numerical model (FEM, CFD, …).
    PhysicalNumerical,
    Other(String),
}

/// A textual equation, e.g. `"dT/dt = (P_in - h*A*(T - T_env)) / C"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Equation {
    pub name: String,
    pub expression: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// How the model can be verified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationCriterion {
    pub name: String,
    /// Expression evaluated by the validation engine on model outputs, e.g.
    /// `"abs(energy_drift) < 1e-6"`.
    pub expression: String,
    #[serde(default)]
    pub source: Option<String>,
}

/// Declarative model description (§3.3: every model declares its inputs,
/// outputs, units, assumptions, validity domain, parameters, dependencies,
/// uncertainties and verification criteria).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub name: String,
    pub version: String,
    pub formalism: Formalism,
    #[serde(default)]
    pub domains: Vec<String>,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// Internal state variables (for dynamic models).
    #[serde(default)]
    pub states: Vec<Port>,
    #[serde(default)]
    pub parameters: Vec<Quantity>,
    #[serde(default)]
    pub equations: Vec<Equation>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub validity_domain: ValidityDomain,
    /// Other models this one depends on.
    #[serde(default)]
    pub dependencies: Vec<ModelId>,
    pub uncertainty: UncertaintyModel,
    #[serde(default)]
    pub verification_criteria: Vec<VerificationCriterion>,
    /// Component that executes this model, if any.
    #[serde(default)]
    pub component: Option<ComponentId>,
    #[serde(default)]
    pub description: Option<String>,
}

impl ModelDescriptor {
    pub fn new(name: &str, version: &str, formalism: Formalism) -> Self {
        Self {
            id: ModelId::deterministic(&format!("model:{name}@{version}")),
            name: name.into(),
            version: version.into(),
            formalism,
            domains: vec![],
            inputs: vec![],
            outputs: vec![],
            states: vec![],
            parameters: vec![],
            equations: vec![],
            assumptions: vec![],
            validity_domain: ValidityDomain::default(),
            dependencies: vec![],
            uncertainty: UncertaintyModel::None,
            verification_criteria: vec![],
            component: None,
            description: None,
        }
    }
    pub fn with_domains(mut self, d: &[&str]) -> Self {
        self.domains = d.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn with_inputs(mut self, p: Vec<Port>) -> Self {
        self.inputs = p;
        self
    }
    pub fn with_outputs(mut self, p: Vec<Port>) -> Self {
        self.outputs = p;
        self
    }
    pub fn with_states(mut self, p: Vec<Port>) -> Self {
        self.states = p;
        self
    }
    pub fn with_parameters(mut self, p: Vec<Quantity>) -> Self {
        self.parameters = p;
        self
    }
    pub fn with_equation(mut self, name: &str, expr: &str) -> Self {
        self.equations.push(Equation {
            name: name.into(),
            expression: expr.into(),
            description: None,
        });
        self
    }
    pub fn with_assumptions(mut self, a: &[&str]) -> Self {
        self.assumptions = a.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn with_component(mut self, c: ComponentId) -> Self {
        self.component = Some(c);
        self
    }
    pub fn with_dependency(mut self, m: ModelId) -> Self {
        self.dependencies.push(m);
        self
    }
    pub fn with_criterion(mut self, name: &str, expr: &str) -> Self {
        self.verification_criteria.push(VerificationCriterion {
            name: name.into(),
            expression: expr.into(),
            source: None,
        });
        self
    }

    /// Dimensional analysis of every equation, using the declared units of
    /// inputs, outputs, states and parameters, `t` in seconds and the
    /// derivative notation `dX/dt` (unit of X per second).  An equation is
    /// `"lhs = rhs"` in `expression`, or — as the model factory writes them —
    /// `name` = lhs and `expression` = rhs.
    pub fn equation_dimension_reports(&self) -> Vec<(String, EquationCheck)> {
        let mut units: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
        for p in self.inputs.iter().chain(&self.outputs).chain(&self.states) {
            units.insert(p.name.clone(), p.unit.clone());
        }
        for q in &self.parameters {
            units
                .entry(q.name.clone())
                .or_insert_with(|| q.unit.clone());
        }
        units.entry("t".into()).or_insert_with(|| "s".into());
        self.equations
            .iter()
            .map(|e| {
                let (lhs, rhs) = match e.expression.split_once('=') {
                    Some((l, r)) if !r.starts_with('=') && !l.ends_with(['<', '>', '!']) => {
                        (l.to_string(), r.to_string())
                    }
                    _ => (e.name.clone(), e.expression.clone()),
                };
                let (lhs, rhs) = (derivatives(&lhs, &mut units), derivatives(&rhs, &mut units));
                let check = match (
                    simorph_validation::dimension_of(&lhs, &units),
                    simorph_validation::dimension_of(&rhs, &units),
                ) {
                    (Ok(a), Ok(b)) if a == b => EquationCheck::Consistent,
                    (Ok(a), Ok(b)) => EquationCheck::Inconsistent(format!(
                        "`{}`: left [{a}] ≠ right [{b}]",
                        e.expression
                    )),
                    (Err(m), _) | (_, Err(m)) if is_dimension_error(&m) => {
                        EquationCheck::Inconsistent(format!("`{}`: {m}", e.expression))
                    }
                    (Err(m), _) | (_, Err(m)) => EquationCheck::Unchecked(m),
                };
                (e.name.clone(), check)
            })
            .collect()
    }

    /// Structural checks: every port must carry a parseable unit, names must
    /// be unique, parameters must carry parseable units.
    pub fn validate_structure(&self) -> simorph_core::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for p in self
            .inputs
            .iter()
            .chain(self.outputs.iter())
            .chain(self.states.iter())
        {
            simorph_units::parse_unit(&p.unit).map_err(|e| {
                simorph_core::SimorphError::Unit(format!(
                    "model `{}` port `{}`: {e}",
                    self.name, p.name
                ))
            })?;
            if !seen.insert(p.name.clone()) {
                return Err(simorph_core::SimorphError::InvalidInput(format!(
                    "model `{}` has duplicate variable `{}`",
                    self.name, p.name
                )));
            }
        }
        for q in &self.parameters {
            simorph_units::parse_unit(&q.unit).map_err(|e| {
                simorph_core::SimorphError::Unit(format!(
                    "model `{}` parameter `{}`: {e}",
                    self.name, q.name
                ))
            })?;
        }
        Ok(())
    }
}

/// Outcome of the dimensional check of one equation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EquationCheck {
    Consistent,
    /// Dimensionally inconsistent: registration is refused.
    Inconsistent(String),
    /// Could not be checked (unknown symbol, unsupported function or
    /// notation); registration proceeds, the reason is reported.
    Unchecked(String),
}

fn is_dimension_error(m: &str) -> bool {
    m.starts_with("sum of [")
        || m.starts_with("exponent has dimension")
        || m.contains("of arguments with different dimensions")
        || m.contains("of a dimensional argument")
}

/// Rewrite `dX/dt` into a symbol `__d_X_dt` whose unit is unit(X)/s.
fn derivatives(expr: &str, units: &mut indexmap::IndexMap<String, String>) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let boundary = i == 0 || !ident(chars[i - 1]);
        if boundary && chars[i] == 'd' {
            let mut j = i + 1;
            while j < chars.len() && ident(chars[j]) {
                j += 1;
            }
            let name: String = chars[i + 1..j].iter().collect();
            let tail: String = chars[j..].iter().take(3).collect();
            let after = chars.get(j + 3).copied();
            if !name.is_empty()
                && tail == "/dt"
                && after.map(|c| !ident(c)).unwrap_or(true)
                && units.contains_key(&name)
            {
                let sym = format!("__d_{name}_dt");
                let u = format!("({})/s", units[&name]);
                units.insert(sym.clone(), u);
                out.push_str(&sym);
                i = j + 3;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
