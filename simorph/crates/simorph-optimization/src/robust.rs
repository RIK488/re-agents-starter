//! Robust optimisation: optimise the expectation or CVaR of an objective
//! under uncertain parameters.

use crate::problem::*;
use simorph_core::{ExecutionControl, Result};
use simorph_uncertainty::{sample_inputs, Design, InputSpec};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RobustCriterion {
    /// Mean over the parameter samples.
    Expectation,
    /// Conditional value at risk: mean of the worst `alpha` fraction.
    Cvar { alpha: f64 },
    /// Worst case over samples.
    WorstCase,
}

/// Wraps a problem `f(x, theta)` with uncertain `theta` into a deterministic
/// problem on `x` by evaluating a fixed sample set of `theta` (common random
/// numbers, so the outer optimiser sees a smooth objective).
pub struct RobustProblem<F: Fn(&[f64], &[f64]) -> Result<f64> + Sync> {
    pub bounds: Vec<(f64, f64)>,
    pub f: F,
    pub theta_samples: Vec<Vec<f64>>,
    pub criterion: RobustCriterion,
}

impl<F: Fn(&[f64], &[f64]) -> Result<f64> + Sync> RobustProblem<F> {
    pub fn new(
        bounds: Vec<(f64, f64)>,
        uncertain: &[InputSpec],
        n_samples: usize,
        seed: u64,
        criterion: RobustCriterion,
        f: F,
    ) -> Result<Self> {
        let theta_samples = sample_inputs(Design::LatinHypercube, uncertain, n_samples, seed)?;
        Ok(Self {
            bounds,
            f,
            theta_samples,
            criterion,
        })
    }
}

impl<F: Fn(&[f64], &[f64]) -> Result<f64> + Sync> ObjectiveFunction for RobustProblem<F> {
    fn dimension(&self) -> usize {
        self.bounds.len()
    }
    fn bounds(&self) -> Vec<(f64, f64)> {
        self.bounds.clone()
    }
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
        let mut vals: Vec<f64> = self
            .theta_samples
            .iter()
            .map(|t| (self.f)(x, t))
            .collect::<Result<_>>()?;
        let v = match self.criterion {
            RobustCriterion::Expectation => vals.iter().sum::<f64>() / vals.len() as f64,
            RobustCriterion::WorstCase => vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            RobustCriterion::Cvar { alpha } => {
                vals.sort_by(|a, b| b.total_cmp(a));
                let k = ((alpha * vals.len() as f64).ceil() as usize).clamp(1, vals.len());
                vals[..k].iter().sum::<f64>() / k as f64
            }
        };
        Ok(Evaluation::scalar(v))
    }
}

/// Optimise a robust problem with CMA-ES (default) and tag the method.
pub fn robust_optimize<F: Fn(&[f64], &[f64]) -> Result<f64> + Sync>(
    p: &RobustProblem<F>,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let mut r = crate::evolutionary::cma_es(
        p,
        None,
        &crate::evolutionary::CmaOptions {
            restarts: 2,
            ..Default::default()
        },
        budget,
        ctl,
    )?;
    r.method = match p.criterion {
        RobustCriterion::Expectation => Method::RobustExpectation,
        RobustCriterion::WorstCase => Method::RobustWorstCase,
        RobustCriterion::Cvar { .. } => Method::RobustCvar,
    };
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::Uncertainty;

    /// Two-peak function: a sharp deep minimum at x=-2 (depth -1.2, width
    /// 0.1) and a wide shallow minimum at x=2 (depth -1, width 1).  Under a
    /// perturbation x + theta with theta ~ N(0, 0.3), the flat minimum is
    /// better in expectation.
    fn two_peak(x: f64) -> f64 {
        -1.2 * (-((x + 2.0) / 0.1).powi(2)).exp() - 1.0 * (-((x - 2.0) / 1.0).powi(2)).exp()
    }

    #[test]
    fn nominal_optimum_is_the_sharp_peak() {
        let p = scalar_problem(vec![(-4.0, 4.0)], |x| two_peak(x[0]));
        let r = crate::evolutionary::cma_es(
            &p,
            None,
            &crate::evolutionary::CmaOptions {
                restarts: 4,
                ..Default::default()
            },
            &Budget::new(4000).seed(1),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!((r.best_x[0] + 2.0).abs() < 0.05, "x={}", r.best_x[0]);
    }

    #[test]
    fn robust_optimum_is_the_flat_peak() {
        let unc = vec![InputSpec::new(
            "theta",
            0.0,
            Uncertainty::Normal { std_dev: 0.3 },
        )];
        let p = RobustProblem::new(
            vec![(-4.0, 4.0)],
            &unc,
            200,
            3,
            RobustCriterion::Expectation,
            |x, t| Ok(two_peak(x[0] + t[0])),
        )
        .unwrap();
        let r =
            robust_optimize(&p, &Budget::new(4000).seed(2), &ExecutionControl::default()).unwrap();
        assert!((r.best_x[0] - 2.0).abs() < 0.2, "x={}", r.best_x[0]);
        assert_eq!(r.method, Method::RobustExpectation);
        let p = RobustProblem::new(
            vec![(-4.0, 4.0)],
            &unc,
            200,
            3,
            RobustCriterion::Cvar { alpha: 0.2 },
            |x, t| Ok(two_peak(x[0] + t[0])),
        )
        .unwrap();
        let r =
            robust_optimize(&p, &Budget::new(4000).seed(2), &ExecutionControl::default()).unwrap();
        assert!((r.best_x[0] - 2.0).abs() < 0.3, "cvar x={}", r.best_x[0]);
    }
}
