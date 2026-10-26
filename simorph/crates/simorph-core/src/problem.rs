//! Problem specification: what the user hands to SIMORPH (§2, §3.1, §3.2).
//!
//! The specification is deliberately domain-agnostic.  Discipline tags are
//! free strings; the knowledge service maps them to plugins.

use crate::ids::{DatasetId, HypothesisId, ProblemId};
use crate::quantity::Quantity;
use crate::tradeoff::TradeoffWeights;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Direction of an objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Minimize,
    Maximize,
    /// Reach a target value (treated as minimising |f - target|).
    Target,
}

/// An objective declared by the problem owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Objective {
    pub name: String,
    pub direction: Direction,
    /// Output variable name of the model this objective reads.
    pub variable: String,
    pub unit: String,
    #[serde(default)]
    pub target: Option<f64>,
    /// Relative weight for scalarisation when the user wants a single score.
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    /// variable <= bound
    UpperBound,
    /// variable >= bound
    LowerBound,
    /// variable == bound (within tolerance)
    Equality,
}

/// A constraint on a model output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Constraint {
    pub name: String,
    pub variable: String,
    pub kind: ConstraintKind,
    pub bound: f64,
    pub unit: String,
    #[serde(default)]
    pub tolerance: f64,
    /// Hard constraints are never violated by an accepted solution; soft
    /// constraints are penalised.
    #[serde(default = "default_true")]
    pub hard: bool,
}

fn default_true() -> bool {
    true
}

/// A quantity that is unknown and that SIMORPH must determine or bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Unknown {
    pub name: String,
    pub unit: String,
    /// Prior interval if any.
    #[serde(default)]
    pub prior: Option<(f64, f64)>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A design variable the optimiser may change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignVariable {
    pub name: String,
    pub unit: String,
    pub low: f64,
    pub high: f64,
    #[serde(default)]
    pub integer: bool,
    #[serde(default)]
    pub initial: Option<f64>,
}

/// Reference to an attached document, dataset, image or file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Attachment {
    pub name: String,
    /// MIME type or extension.
    pub media_type: String,
    /// Path or URI.
    pub location: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub dataset_id: Option<DatasetId>,
}

/// Hypothesis formulated by the director or the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Hypothesis {
    pub id: HypothesisId,
    pub statement: String,
    /// What observation would falsify it.
    pub falsification_criterion: String,
    #[serde(default)]
    pub status: HypothesisStatus,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisStatus {
    #[default]
    Open,
    Supported,
    Refuted,
    Inconclusive,
    Abandoned,
}

/// The full problem specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProblemSpec {
    pub id: ProblemId,
    pub title: String,
    /// Natural-language description.
    pub description: String,
    /// Discipline tags (free-form, e.g. "aerospace", "thermal", "power").
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    /// Known quantities with units and uncertainties.
    #[serde(default)]
    pub knowns: Vec<Quantity>,
    #[serde(default)]
    pub unknowns: Vec<Unknown>,
    #[serde(default)]
    pub design_variables: Vec<DesignVariable>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub hypotheses: Vec<Hypothesis>,
    /// Weights for the precision/time/cost/risk/information trade-off.
    #[serde(default)]
    pub tradeoff_weights: TradeoffWeights,
    /// Budget caps declared by the owner.
    #[serde(default)]
    pub budget: Budget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Budget {
    pub max_wall_time_s: f64,
    pub max_runs: u64,
    pub max_cost: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self { max_wall_time_s: 3600.0, max_runs: 10_000, max_cost: f64::INFINITY }
    }
}

impl ProblemSpec {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: ProblemId::new(),
            title: title.into(),
            description: description.into(),
            domains: vec![],
            objectives: vec![],
            constraints: vec![],
            knowns: vec![],
            unknowns: vec![],
            design_variables: vec![],
            attachments: vec![],
            hypotheses: vec![],
            tradeoff_weights: TradeoffWeights::default(),
            budget: Budget::default(),
        }
    }

    /// Basic consistency checks that do not need unit parsing.
    pub fn validate_structure(&self) -> crate::Result<()> {
        if self.title.trim().is_empty() {
            return Err(crate::SimorphError::InvalidInput("problem title empty".into()));
        }
        for d in &self.design_variables {
            if d.low > d.high {
                return Err(crate::SimorphError::InvalidInput(format!(
                    "design variable `{}` has low > high",
                    d.name
                )));
            }
        }
        for o in &self.objectives {
            if o.direction == Direction::Target && o.target.is_none() {
                return Err(crate::SimorphError::InvalidInput(format!(
                    "objective `{}` is Target but has no target value",
                    o.name
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_objective_needs_value() {
        let mut p = ProblemSpec::new("t", "d");
        p.objectives.push(Objective {
            name: "o".into(),
            direction: Direction::Target,
            variable: "x".into(),
            unit: "m".into(),
            target: None,
            weight: 1.0,
        });
        assert!(p.validate_structure().is_err());
    }
}
