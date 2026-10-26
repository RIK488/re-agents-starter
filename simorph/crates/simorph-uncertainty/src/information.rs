//! Value of information and fidelity selection (§8).

use crate::propagation::evaluate_samples;
use crate::sampling::{sample_inputs, Design, InputSpec};
use crate::stats::{mean, variance};
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError, Tradeoff, TradeoffWeights};

/// Expected value of perfect information for a finite decision set.
///
/// `utilities[k][j]` is the utility of decision `j` under parameter sample
/// `k`.  EVPI = E[max_j u] − max_j E[u].
pub fn evpi(utilities: &[Vec<f64>]) -> f64 {
    if utilities.is_empty() {
        return 0.0;
    }
    let m = utilities[0].len();
    let e_max = mean(
        &utilities
            .iter()
            .map(|u| u.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
            .collect::<Vec<_>>(),
    );
    let max_e = (0..m)
        .map(|j| mean(&utilities.iter().map(|u| u[j]).collect::<Vec<_>>()))
        .fold(f64::NEG_INFINITY, f64::max);
    e_max - max_e
}

/// Expected variance reduction of the output if input `i` were measured
/// exactly: Var(E[Y | X_i]), estimated by binning the samples of X_i.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VarianceReduction {
    pub name: String,
    /// Var(E[Y|X_i]) in output units squared.
    pub expected_reduction: f64,
    /// Fraction of total variance (≈ first-order Sobol index).
    pub fraction: f64,
}

/// Preposterior analysis: which input is most worth measuring?
pub fn expected_variance_reduction<F>(
    inputs: &[InputSpec],
    n: usize,
    seed: u64,
    output_index: usize,
    bins: usize,
    f: F,
    ctl: &ExecutionControl,
) -> Result<Vec<VarianceReduction>>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    let xs = sample_inputs(Design::LatinHypercube, inputs, n, seed)?;
    let (kept, out, _) = evaluate_samples(&xs, f, ctl)?;
    if kept.len() < bins * 2 {
        return Err(SimorphError::Numerical(
            "too few successful evaluations".into(),
        ));
    }
    let y: Vec<f64> = out.iter().map(|o| o[output_index]).collect();
    let total = variance(&y);
    let mut res = vec![];
    for (i, spec) in inputs.iter().enumerate() {
        let mut idx: Vec<usize> = (0..kept.len()).collect();
        idx.sort_by(|&a, &b| kept[a][i].partial_cmp(&kept[b][i]).unwrap());
        let per = kept.len() / bins;
        let mut cond_means = Vec::with_capacity(bins);
        for b in 0..bins {
            let slice = &idx[b * per..if b == bins - 1 {
                kept.len()
            } else {
                (b + 1) * per
            }];
            cond_means.push(mean(&slice.iter().map(|&k| y[k]).collect::<Vec<_>>()));
        }
        let v = variance(&cond_means) * (bins as f64 - 1.0) / bins as f64; // population variance of bin means
        res.push(VarianceReduction {
            name: spec.name.clone(),
            expected_reduction: v,
            fraction: if total > 0.0 { v / total } else { 0.0 },
        });
    }
    Ok(res)
}

/// A fidelity level available for a simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FidelityLevel {
    pub name: String,
    /// Fidelity value passed to `TaskRequest::fidelity` (0..1).
    pub fidelity: f64,
    /// Expected relative error of the output at this level.
    pub expected_rel_error: f64,
    /// Expected wall-clock seconds.
    pub compute_time_s: f64,
    /// Monetary / resource cost.
    pub cost: f64,
    /// Risk of failure or misleading result (0..1).
    pub risk: f64,
}

/// Decision produced by the fidelity selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FidelityDecision {
    pub chosen: FidelityLevel,
    pub tradeoff: Tradeoff,
    pub utility: f64,
    /// All candidates with their utilities, for auditability.
    pub ranking: Vec<(String, f64, bool)>,
    pub justification: String,
}

/// Chooses the fidelity level for the next simulation (§8).
pub struct FidelitySelector {
    pub levels: Vec<FidelityLevel>,
    pub weights: TradeoffWeights,
}

impl FidelitySelector {
    pub fn new(levels: Vec<FidelityLevel>, weights: TradeoffWeights) -> Self {
        Self { levels, weights }
    }

    /// Pick the level with the best trade-off utility among those meeting
    /// `target_rel_error`.  `information_value` is the expected information
    /// gain of the experiment (e.g. from [`expected_variance_reduction`]),
    /// scaled by how much of it the level's precision captures.
    pub fn select(
        &self,
        target_rel_error: f64,
        information_value: f64,
    ) -> Result<FidelityDecision> {
        if self.levels.is_empty() {
            return Err(SimorphError::InvalidInput("no fidelity levels".into()));
        }
        let mut ranking = vec![];
        let mut best: Option<(usize, Tradeoff, f64)> = None;
        for (i, l) in self.levels.iter().enumerate() {
            let meets = l.expected_rel_error <= target_rel_error;
            let precision = (1.0 - l.expected_rel_error).clamp(0.0, 1.0);
            let t = Tradeoff {
                precision,
                compute_time_s: l.compute_time_s,
                cost: l.cost,
                risk: l.risk,
                information_value: information_value * precision,
            };
            let u = t.utility(&self.weights);
            ranking.push((l.name.clone(), u, meets));
            if meets && best.map_or(true, |(_, _, bu)| u > bu) {
                best = Some((i, t, u));
            }
        }
        let (i, tradeoff, utility) = match best {
            Some(b) => b,
            None => {
                // No level meets the target: choose the most precise one and say so.
                let i = (0..self.levels.len())
                    .min_by(|&a, &b| {
                        self.levels[a]
                            .expected_rel_error
                            .partial_cmp(&self.levels[b].expected_rel_error)
                            .unwrap()
                    })
                    .unwrap();
                let l = &self.levels[i];
                let precision = (1.0 - l.expected_rel_error).clamp(0.0, 1.0);
                let t = Tradeoff {
                    precision,
                    compute_time_s: l.compute_time_s,
                    cost: l.cost,
                    risk: l.risk,
                    information_value: information_value * precision,
                };
                let u = t.utility(&self.weights);
                return Ok(FidelityDecision {
                    chosen: l.clone(),
                    tradeoff: t,
                    utility: u,
                    ranking,
                    justification: format!(
                        "no fidelity level meets target relative error {target_rel_error}; chose most precise level `{}` (error {}). Real data or a better model is required.",
                        l.name, l.expected_rel_error
                    ),
                });
            }
        };
        ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(FidelityDecision {
            chosen: self.levels[i].clone(),
            tradeoff,
            utility,
            justification: format!(
                "level `{}` meets target relative error {target_rel_error} with the best precision/time/cost/risk/information utility ({utility:.3})",
                self.levels[i].name
            ),
            ranking,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evpi_of_linear_decision_matches_analytic() {
        // Two decisions: d0 gives utility 0, d1 gives theta, theta ~ U(-1, 1).
        // max_j E[u] = max(0, 0) = 0; E[max(0, theta)] = 1/4.  EVPI = 0.25.
        let inputs = vec![InputSpec::uniform("theta", -1.0, 1.0)];
        let xs = sample_inputs(Design::LatinHypercube, &inputs, 20000, 1).unwrap();
        let u: Vec<Vec<f64>> = xs.iter().map(|x| vec![0.0, x[0]]).collect();
        assert!((evpi(&u) - 0.25).abs() < 0.01);
    }

    #[test]
    fn variance_reduction_ranks_dominant_input() {
        let inputs = vec![
            InputSpec::normal("big", 0.0, 3.0),
            InputSpec::normal("small", 0.0, 1.0),
        ];
        let r = expected_variance_reduction(
            &inputs,
            4000,
            2,
            0,
            20,
            |x| Ok(vec![x[0] + x[1]]),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r[0].fraction > r[1].fraction);
        assert!((r[0].fraction - 0.9).abs() < 0.08, "{}", r[0].fraction);
    }

    #[test]
    fn fidelity_selector_prefers_cheapest_adequate_level() {
        let levels = vec![
            FidelityLevel {
                name: "coarse".into(),
                fidelity: 0.2,
                expected_rel_error: 0.2,
                compute_time_s: 1.0,
                cost: 1.0,
                risk: 0.1,
            },
            FidelityLevel {
                name: "medium".into(),
                fidelity: 0.6,
                expected_rel_error: 0.05,
                compute_time_s: 60.0,
                cost: 10.0,
                risk: 0.05,
            },
            FidelityLevel {
                name: "fine".into(),
                fidelity: 1.0,
                expected_rel_error: 0.01,
                compute_time_s: 3600.0,
                cost: 100.0,
                risk: 0.05,
            },
        ];
        let sel = FidelitySelector::new(levels, TradeoffWeights::default());
        let d = sel.select(0.1, 1.0).unwrap();
        assert_eq!(d.chosen.name, "medium");
        let d = sel.select(0.5, 1.0).unwrap();
        assert_eq!(d.chosen.name, "coarse");
        let d = sel.select(0.001, 1.0).unwrap();
        assert_eq!(d.chosen.name, "fine");
        assert!(d.justification.contains("no fidelity level meets"));
    }
}
