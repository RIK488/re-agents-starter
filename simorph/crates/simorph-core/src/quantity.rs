//! Physical quantities with units and uncertainty.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Uncertainty model attached to a quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Uncertainty {
    /// No uncertainty declared (NOT the same as zero uncertainty).
    Undeclared,
    /// Exactly known (e.g. defined constants, integer counts).
    Exact,
    /// Gaussian with standard deviation.
    Normal { std_dev: f64 },
    /// Uniform in [low, high].
    Uniform { low: f64, high: f64 },
    /// Log-normal with parameters of the underlying normal.
    LogNormal { mu: f64, sigma: f64 },
    /// Triangular distribution.
    Triangular { low: f64, mode: f64, high: f64 },
    /// Empirical samples.
    Samples { values: Vec<f64> },
    /// Interval with no distributional assumption.
    Interval { low: f64, high: f64 },
}

impl Default for Uncertainty {
    fn default() -> Self {
        Uncertainty::Undeclared
    }
}

/// A scalar value with a unit expression (parsed by `simorph-units`) and an
/// uncertainty model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Quantity {
    pub name: String,
    pub value: f64,
    /// Unit expression, e.g. `"kg*m/s^2"`, `"W"`, `"1"` for dimensionless.
    pub unit: String,
    #[serde(default)]
    pub uncertainty: Uncertainty,
    #[serde(default)]
    pub description: Option<String>,
}

impl Quantity {
    pub fn new(name: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        Self { name: name.into(), value, unit: unit.into(), uncertainty: Uncertainty::Undeclared, description: None }
    }
    pub fn with_uncertainty(mut self, u: Uncertainty) -> Self {
        self.uncertainty = u;
        self
    }
    pub fn dimensionless(name: impl Into<String>, value: f64) -> Self {
        Self::new(name, value, "1")
    }
}

/// A named variable port of a model/component (input or output).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Port {
    pub name: String,
    pub unit: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Shape of the value: `[]` scalar, `[3]` vector, `[3,3]` matrix …
    #[serde(default)]
    pub shape: Vec<usize>,
    /// Optional validity range in the declared unit.
    #[serde(default)]
    pub range: Option<(f64, f64)>,
}

impl Port {
    pub fn scalar(name: impl Into<String>, unit: impl Into<String>) -> Self {
        Self { name: name.into(), unit: unit.into(), description: None, shape: vec![], range: None }
    }
    pub fn with_range(mut self, low: f64, high: f64) -> Self {
        self.range = Some((low, high));
        self
    }
}
