//! Simulated annealing.

use crate::problem::*;
use rand::Rng as _;
use rand_distr::{Distribution, StandardNormal};
use simorph_core::{ExecutionControl, Result};
use simorph_uncertainty::seeded;

#[derive(Debug, Clone)]
pub struct SaOptions {
    pub t0: f64,
    /// Geometric cooling factor per iteration.
    pub cooling: f64,
    /// Initial step as fraction of box width.
    pub step_frac: f64,
}

impl Default for SaOptions {
    fn default() -> Self {
        Self {
            t0: 1.0,
            cooling: 0.995,
            step_frac: 0.1,
        }
    }
}

pub fn simulated_annealing(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    opts: &SaOptions,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    crate::problem::guarded(p, budget, Method::SimulatedAnnealing, |g| {
        simulated_annealing_impl(g, x0, opts, budget, ctl)
    })
}

fn simulated_annealing_impl(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    opts: &SaOptions,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let bounds = p.bounds();
    let c = Counted::new(p);
    let mut rng = seeded(budget.seed);
    let penalty = 1e6;
    let mut x = match x0 {
        Some(x) => x.to_vec(),
        None => random_point(&bounds, &mut rng),
    };
    let mut fx = c.eval_scalar(&x, penalty)?;
    let mut best_x = x.clone();
    let mut best_f = fx;
    let mut t = opts.t0 * fx.abs().max(1.0);
    let mut step: Vec<f64> = bounds
        .iter()
        .map(|(lo, hi)| opts.step_frac * (hi - lo))
        .collect();
    let mut history = vec![];
    let mut iter = 0;
    let mut accepted = 0usize;
    while c.count() < budget.max_evaluations && iter < budget.max_iterations {
        ctl.check()?;
        iter += 1;
        let mut xn = x.clone();
        for j in 0..d {
            let z: f64 = StandardNormal.sample(&mut rng);
            xn[j] += step[j] * z;
        }
        clamp_to_bounds(&mut xn, &bounds);
        let fnew = c.eval_scalar(&xn, penalty)?;
        if fnew <= fx || rng.gen::<f64>() < ((fx - fnew) / t).exp() {
            x = xn;
            fx = fnew;
            accepted += 1;
        }
        if fx < best_f {
            best_f = fx;
            best_x = x.clone();
            history.push(HistoryPoint {
                evaluations: c.count(),
                best_f,
            });
        }
        t *= opts.cooling;
        // adapt step toward ~40% acceptance every 100 iterations
        if iter % 100 == 0 {
            let rate = accepted as f64 / 100.0;
            let factor = if rate > 0.5 {
                1.2
            } else if rate < 0.3 {
                0.8
            } else {
                1.0
            };
            for s in &mut step {
                *s *= factor;
            }
            accepted = 0;
        }
    }
    let best_eval = p.evaluate(&best_x)?;
    Ok(OptimizationResult {
        method: Method::SimulatedAnnealing,
        best_x,
        best_f,
        best_eval,
        evaluations: c.count(),
        iterations: iter,
        converged: false,
        history,
        message: "annealing schedule completed".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::*;

    #[test]
    fn annealing_sphere_2d() {
        let p = sphere_problem(2);
        let r = simulated_annealing(
            &p,
            Some(&[4.0, -4.0]),
            &SaOptions::default(),
            &Budget::new(20_000).iterations(20_000).seed(1),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-3, "f={}", r.best_f);
    }
}
