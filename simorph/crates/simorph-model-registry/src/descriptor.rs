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
