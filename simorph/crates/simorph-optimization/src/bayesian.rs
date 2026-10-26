//! Bayesian optimisation with a Gaussian process surrogate and Expected
//! Improvement.

use crate::problem::*;
use nalgebra::{DMatrix, DVector};
use rand::Rng as _;
use simorph_core::{ExecutionControl, Result, SimorphError};
use simorph_uncertainty::seeded;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kernel {
    Rbf,
    Matern52,
}

/// Gaussian process regression on normalised inputs in [0,1]^d.
#[derive(Debug, Clone)]
pub struct GaussianProcess {
    pub kernel: Kernel,
    pub length_scale: f64,
    pub signal_var: f64,
    pub noise: f64,
    xs: Vec<Vec<f64>>,
    y_mean: f64,
    y_std: f64,
    alpha: DVector<f64>,
    chol: Option<nalgebra::Cholesky<f64, nalgebra::Dyn>>,
}

impl GaussianProcess {
    fn k(&self, a: &[f64], b: &[f64]) -> f64 {
        let r2: f64 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>()
            / (self.length_scale * self.length_scale);
        match self.kernel {
            Kernel::Rbf => self.signal_var * (-0.5 * r2).exp(),
            Kernel::Matern52 => {
                let r = r2.sqrt() * 5f64.sqrt();
                self.signal_var * (1.0 + r + r * r / 3.0) * (-r).exp()
            }
        }
    }

    fn fit_fixed(
        kernel: Kernel,
        ls: f64,
        noise: f64,
        xs: &[Vec<f64>],
        ys: &[f64],
    ) -> Result<(Self, f64)> {
        let n = xs.len();
        let y_mean = ys.iter().sum::<f64>() / n as f64;
        let y_std = (ys.iter().map(|v| (v - y_mean).powi(2)).sum::<f64>() / n as f64)
            .sqrt()
            .max(1e-12);
        let yn: Vec<f64> = ys.iter().map(|v| (v - y_mean) / y_std).collect();
        let mut gp = Self {
            kernel,
            length_scale: ls,
            signal_var: 1.0,
            noise,
            xs: xs.to_vec(),
            y_mean,
            y_std,
            alpha: DVector::zeros(n),
            chol: None,
        };
        let mut kmat = DMatrix::from_fn(n, n, |i, j| gp.k(&xs[i], &xs[j]));
        for i in 0..n {
            kmat[(i, i)] += noise;
        }
        let chol = nalgebra::Cholesky::new(kmat).ok_or_else(|| {
            SimorphError::Numerical("GP kernel matrix not positive definite".into())
        })?;
        let y = DVector::from_column_slice(&yn);
        let alpha = chol.solve(&y);
        // log marginal likelihood
        let logdet: f64 = chol.l().diagonal().iter().map(|v| v.ln()).sum::<f64>() * 2.0;
        let lml = -0.5 * y.dot(&alpha)
            - 0.5 * logdet
            - 0.5 * n as f64 * (2.0 * std::f64::consts::PI).ln();
        gp.alpha = alpha;
        gp.chol = Some(chol);
        Ok((gp, lml))
    }

    /// Fit with hyperparameters chosen by maximum marginal likelihood over a
    /// log-spaced grid of length scales and noise levels.
    pub fn fit(kernel: Kernel, xs: &[Vec<f64>], ys: &[f64]) -> Result<Self> {
        if xs.len() < 2 {
            return Err(SimorphError::InvalidInput(
                "GP needs at least two points".into(),
            ));
        }
        let mut best: Option<(Self, f64)> = None;
        for &ls in &[0.05, 0.1, 0.2, 0.35, 0.5, 0.8, 1.2, 2.0] {
            for &noise in &[1e-8, 1e-6, 1e-4, 1e-2] {
                if let Ok((gp, lml)) = Self::fit_fixed(kernel, ls, noise, xs, ys) {
                    if best.as_ref().map_or(true, |(_, b)| lml > *b) {
                        best = Some((gp, lml));
                    }
                }
            }
        }
        best.map(|(g, _)| g)
            .ok_or_else(|| SimorphError::Numerical("GP fit failed for all hyperparameters".into()))
    }

    /// Predictive mean and variance (in original units).
    pub fn predict(&self, x: &[f64]) -> (f64, f64) {
        let kx = DVector::from_iterator(self.xs.len(), self.xs.iter().map(|xi| self.k(xi, x)));
        let mean = kx.dot(&self.alpha);
        let v = self.chol.as_ref().unwrap().solve(&kx);
        let var = (self.k(x, x) - kx.dot(&v)).max(0.0);
        (
            mean * self.y_std + self.y_mean,
            var * self.y_std * self.y_std,
        )
    }
}

fn norm_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / 2f64.sqrt()))
}

fn erf(x: f64) -> f64 {
    // Abramowitz–Stegun 7.1.26, |err| < 1.5e-7
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    if x >= 0.0 {
        y
    } else {
        -y
    }
}

/// Expected improvement for minimisation.
pub fn expected_improvement(mean: f64, var: f64, best: f64, xi: f64) -> f64 {
    let sd = var.sqrt();
    if sd < 1e-12 {
        return (best - mean - xi).max(0.0);
    }
    let z = (best - mean - xi) / sd;
    let pdf = (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt();
    (best - mean - xi) * norm_cdf(z) + sd * pdf
}

#[derive(Debug, Clone)]
pub struct BoOptions {
    pub kernel: Kernel,
    pub initial_points: usize,
    /// Random candidates for acquisition optimisation.
    pub acquisition_candidates: usize,
    pub xi: f64,
}

impl Default for BoOptions {
    fn default() -> Self {
        Self {
            kernel: Kernel::Matern52,
            initial_points: 0,
            acquisition_candidates: 2000,
            xi: 0.01,
        }
    }
}

/// Bayesian optimisation: LHS initial design, GP surrogate, EI maximised by
/// random multistart followed by local coordinate refinement.
pub fn bayesian_optimization(
    p: &dyn ObjectiveFunction,
    opts: &BoOptions,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let bounds = p.bounds();
    let c = Counted::new(p);
    let penalty = 1e6;
    let mut rng = seeded(budget.seed);
    let n0 = if opts.initial_points == 0 {
        (2 * d + 2).min(budget.max_evaluations)
    } else {
        opts.initial_points
    };
    let to_unit = |x: &[f64]| -> Vec<f64> {
        x.iter()
            .zip(&bounds)
            .map(|(v, (lo, hi))| if hi > lo { (v - lo) / (hi - lo) } else { 0.5 })
            .collect()
    };
    let from_unit = |u: &[f64]| -> Vec<f64> {
        u.iter()
            .zip(&bounds)
            .map(|(v, (lo, hi))| lo + v * (hi - lo))
            .collect()
    };
    let mut xs_unit: Vec<Vec<f64>> = simorph_uncertainty::latin_hypercube(n0, d, budget.seed);
    let mut ys: Vec<f64> = vec![];
    for u in &xs_unit {
        ctl.check()?;
        ys.push(c.eval_scalar(&from_unit(u), penalty)?);
    }
    let mut history = vec![];
    let mut iter = 0;
    while c.count() < budget.max_evaluations && iter < budget.max_iterations {
        ctl.check()?;
        iter += 1;
        let best = ys.iter().cloned().fold(f64::INFINITY, f64::min);
        history.push(HistoryPoint {
            evaluations: c.count(),
            best_f: best,
        });
        let gp = GaussianProcess::fit(opts.kernel, &xs_unit, &ys)?;
        let ei = |u: &[f64]| {
            let (m, v) = gp.predict(u);
            expected_improvement(m, v, best, opts.xi)
        };
        // multistart random candidates
        let mut best_u: Vec<f64> = (0..d).map(|_| rng.gen::<f64>()).collect();
        let mut best_ei = ei(&best_u);
        for _ in 0..opts.acquisition_candidates {
            let u: Vec<f64> = (0..d).map(|_| rng.gen::<f64>()).collect();
            let e = ei(&u);
            if e > best_ei {
                best_ei = e;
                best_u = u;
            }
        }
        // local refinement by shrinking coordinate perturbations
        let mut step = 0.1;
        for _ in 0..30 {
            let mut improved = false;
            for j in 0..d {
                for sgn in [-1.0, 1.0] {
                    let mut u = best_u.clone();
                    u[j] = (u[j] + sgn * step).clamp(0.0, 1.0);
                    let e = ei(&u);
                    if e > best_ei {
                        best_ei = e;
                        best_u = u;
                        improved = true;
                    }
                }
            }
            if !improved {
                step *= 0.5;
            }
        }
        // avoid duplicate points (zero-variance Cholesky issues)
        if xs_unit
            .iter()
            .any(|x| x.iter().zip(&best_u).all(|(a, b)| (a - b).abs() < 1e-9))
        {
            best_u = (0..d).map(|_| rng.gen::<f64>()).collect();
        }
        let y = c.eval_scalar(&from_unit(&best_u), penalty)?;
        xs_unit.push(best_u);
        ys.push(y);
    }
    let bi = (0..ys.len())
        .min_by(|&a, &b| ys[a].partial_cmp(&ys[b]).unwrap())
        .unwrap();
    let best_x = from_unit(&xs_unit[bi]);
    let best_eval = p.evaluate(&best_x)?;
    let _ = to_unit;
    Ok(OptimizationResult {
        method: Method::BayesianOptimization,
        best_x,
        best_f: ys[bi],
        best_eval,
        evaluations: c.count(),
        iterations: iter,
        converged: false,
        history,
        message: "evaluation budget exhausted".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gp_interpolates_training_data() {
        let xs: Vec<Vec<f64>> = (0..8).map(|i| vec![i as f64 / 7.0]).collect();
        let ys: Vec<f64> = xs.iter().map(|x| (6.0 * x[0]).sin()).collect();
        let gp = GaussianProcess::fit(Kernel::Rbf, &xs, &ys).unwrap();
        for (x, y) in xs.iter().zip(&ys) {
            let (m, v) = gp.predict(x);
            assert!((m - y).abs() < 1e-2, "m={m} y={y}");
            assert!(v < 1e-2);
        }
        let (_, v_far) = gp.predict(&[0.5 / 7.0]);
        let (_, v_on) = gp.predict(&[0.0]);
        assert!(v_far > v_on);
    }

    #[test]
    fn bo_beats_random_search_on_six_hump_camel() {
        // Six-hump camel in [-3,3]x[-2,2], global minimum -1.0316.
        let f = |x: &[f64]| {
            (4.0 - 2.1 * x[0].powi(2) + x[0].powi(4) / 3.0) * x[0].powi(2)
                + x[0] * x[1]
                + (-4.0 + 4.0 * x[1].powi(2)) * x[1].powi(2)
        };
        let p = scalar_problem(vec![(-3.0, 3.0), (-2.0, 2.0)], f);
        let seeds = 4;
        let mut bo_sum = 0.0;
        let mut rs_sum = 0.0;
        for s in 0..seeds {
            let bo = bayesian_optimization(
                &p,
                &BoOptions::default(),
                &Budget::new(40).seed(11 + s),
                &ExecutionControl::default(),
            )
            .unwrap();
            assert_eq!(bo.evaluations, 40);
            assert!(bo.best_f < -0.5, "bo seed {s}: {}", bo.best_f);
            bo_sum += bo.best_f;
            rs_sum += random_search(
                &p,
                &Budget::new(40).seed(100 + s),
                &ExecutionControl::default(),
            )
            .unwrap()
            .best_f;
        }
        let (bo_avg, rs_avg) = (bo_sum / seeds as f64, rs_sum / seeds as f64);
        assert!(bo_avg < rs_avg - 0.1, "bo_avg={bo_avg} rs_avg={rs_avg}");
        assert!(bo_avg < -0.85, "bo_avg={bo_avg}");
    }

    #[test]
    fn bo_1d() {
        let p = scalar_problem(vec![(0.0, 10.0)], |x| {
            (x[0] - 7.3).powi(2) + (3.0 * x[0]).sin()
        });
        let r = bayesian_optimization(
            &p,
            &BoOptions::default(),
            &Budget::new(25).seed(2),
            &ExecutionControl::default(),
        )
        .unwrap();
        // true minimum is ≈ -0.75 near x ≈ 7.75
        assert!(r.best_f < -0.7, "f={} x={:?}", r.best_f, r.best_x);
    }
}
