//! Measurable trade-off between precision, compute time, cost, risk and
//! information value (§8).  The scientific director scores candidate
//! experiments with this structure; the scores are recorded so that the
//! optimisation of the trade-off is auditable.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tradeoff {
    /// Expected relative precision (0 = useless, 1 = exact).
    pub precision: f64,
    /// Expected wall-clock seconds.
    pub compute_time_s: f64,
    /// Monetary or resource cost in arbitrary consistent units.
    pub cost: f64,
    /// Probability-like risk of failure or of misleading results (0–1).
    pub risk: f64,
    /// Expected reduction of uncertainty on the decision variable, in
    /// nats or any consistent information measure.
    pub information_value: f64,
}

/// Weights for scoring a [`Tradeoff`].  Weights are declared by the problem
/// owner, not by the director, so that the director cannot move the goal
/// posts of its own evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TradeoffWeights {
    pub precision: f64,
    pub compute_time: f64,
    pub cost: f64,
    pub risk: f64,
    pub information_value: f64,
}

impl Default for TradeoffWeights {
    fn default() -> Self {
        Self {
            precision: 1.0,
            compute_time: 1.0,
            cost: 1.0,
            risk: 1.0,
            information_value: 1.0,
        }
    }
}

impl Tradeoff {
    /// Scalar utility; higher is better.  Time and cost are penalised on a
    /// log scale so that a 10x cheaper experiment is worth a constant bonus
    /// regardless of absolute magnitude.
    pub fn utility(&self, w: &TradeoffWeights) -> f64 {
        w.information_value * self.information_value + w.precision * self.precision
            - w.compute_time * (1.0 + self.compute_time_s.max(0.0)).ln()
            - w.cost * (1.0 + self.cost.max(0.0)).ln()
            - w.risk * self.risk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cheaper_experiment_with_same_information_scores_higher() {
        let w = TradeoffWeights::default();
        let a = Tradeoff {
            precision: 0.9,
            compute_time_s: 10.0,
            cost: 1.0,
            risk: 0.1,
            information_value: 2.0,
        };
        let b = Tradeoff {
            compute_time_s: 1000.0,
            ..a
        };
        assert!(a.utility(&w) > b.utility(&w));
    }
}
