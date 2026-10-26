//! Differential evolution and CMA-ES with native constraint handling
//! (Deb's feasibility rules).

use crate::constraints::deb_compare;
use crate::problem::*;
use nalgebra::{DMatrix, DVector, SymmetricEigen};
use rand::Rng as _;
use rand_distr::{Distribution, StandardNormal};
use simorph_core::{ExecutionControl, Result, SimorphError};
use simorph_uncertainty::seeded;
use std::cmp::Ordering;

/// Options for differential evolution (DE/rand/1/bin with jitter).
#[derive(Debug, Clone)]
pub struct DeOptions {
    pub population: usize,
    /// Differential weight F.
    pub f: f64,
    /// Crossover rate CR.
    pub cr: f64,
    /// Number of restarts from random populations when stagnating.
    pub restarts: usize,
}

impl Default for DeOptions {
    fn default() -> Self {
        Self {
            population: 0,
            f: 0.8,
            cr: 0.9,
            restarts: 0,
        }
    }
}

fn round_integers(x: &mut [f64], ints: &[usize]) {
    for &i in ints {
        x[i] = x[i].round();
    }
}

/// Differential evolution.  Constraints are handled with Deb's rules.
pub fn differential_evolution(
    p: &dyn ObjectiveFunction,
    opts: &DeOptions,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let bounds = p.bounds();
    let ints = p.integer_variables();
    let np = if opts.population == 0 {
        (10 * d).max(20)
    } else {
        opts.population
    };
    let c = Counted::new(p);
    let mut rng = seeded(budget.seed);
    let mut history = vec![];
    let mut global_best: Option<(Vec<f64>, Evaluation)> = None;
    let mut iterations = 0;
    let mut converged = false;
    'restart: for _restart in 0..=opts.restarts {
        let mut pop: Vec<Vec<f64>> = (0..np)
            .map(|_| {
                let mut x = random_point(&bounds, &mut rng);
                round_integers(&mut x, &ints);
                x
            })
            .collect();
        let mut evals: Vec<Evaluation> = Vec::with_capacity(np);
        for x in &pop {
            ctl.check()?;
            evals.push(c.eval(x)?);
            if c.count() >= budget.max_evaluations {
                break;
            }
        }
        if evals.len() < np {
            pop.truncate(evals.len());
        }
        let mut stagnation = 0usize;
        let mut last_best = f64::INFINITY;
        while c.count() + pop.len() <= budget.max_evaluations && iterations < budget.max_iterations
        {
            ctl.check()?;
            iterations += 1;
            let n = pop.len();
            if n < 4 {
                break;
            }
            for i in 0..n {
                let mut idx = [0usize; 3];
                for k in 0..3 {
                    loop {
                        let r = rng.gen_range(0..n);
                        if r != i && !idx[..k].contains(&r) {
                            idx[k] = r;
                            break;
                        }
                    }
                }
                let fw = opts.f * (0.9 + 0.2 * rng.gen::<f64>());
                let jrand = rng.gen_range(0..d);
                let mut trial = pop[i].clone();
                for j in 0..d {
                    if rng.gen::<f64>() < opts.cr || j == jrand {
                        trial[j] = pop[idx[0]][j] + fw * (pop[idx[1]][j] - pop[idx[2]][j]);
                    }
                }
                clamp_to_bounds(&mut trial, &bounds);
                round_integers(&mut trial, &ints);
                let te = c.eval(&trial)?;
                if deb_compare(&te, &evals[i]) != Ordering::Greater {
                    pop[i] = trial;
                    evals[i] = te;
                }
            }
            let bi = (0..n)
                .min_by(|&a, &b| deb_compare(&evals[a], &evals[b]))
                .unwrap();
            let bf = scalarize(&evals[bi], 1e6);
            history.push(HistoryPoint {
                evaluations: c.count(),
                best_f: bf,
            });
            match &global_best {
                Some((_, ge)) if deb_compare(ge, &evals[bi]) != Ordering::Greater => {}
                _ => global_best = Some((pop[bi].clone(), evals[bi].clone())),
            }
            // convergence: population spread
            let spread = (0..d)
                .map(|j| {
                    let (mn, mx) = pop
                        .iter()
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
                            (a.min(x[j]), b.max(x[j]))
                        });
                    mx - mn
                })
                .fold(0.0, f64::max);
            let fspread = evals
                .iter()
                .map(|e| e.objectives[0])
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                    (a.min(v), b.max(v))
                });
            if spread < budget.x_tol && (fspread.1 - fspread.0).abs() < budget.f_tol {
                converged = true;
                continue 'restart;
            }
            if (last_best - bf).abs() < 1e-12 {
                stagnation += 1;
            } else {
                stagnation = 0;
            }
            last_best = bf;
            if stagnation > 50 * d {
                continue 'restart;
            }
        }
        break;
    }
    let (best_x, best_eval) =
        global_best.ok_or_else(|| SimorphError::Numerical("DE produced no evaluation".into()))?;
    Ok(OptimizationResult {
        method: Method::DifferentialEvolution,
        best_f: scalarize(&best_eval, 1e6),
        best_x,
        best_eval,
        evaluations: c.count(),
        iterations,
        converged,
        history,
        message: if converged {
            "population converged".into()
        } else {
            "budget exhausted".into()
        },
    })
}

/// CMA-ES options.
#[derive(Debug, Clone)]
pub struct CmaOptions {
    /// Initial step size as a fraction of the box width (default 0.3).
    pub sigma_frac: f64,
    /// Population size λ (0 = default 4 + floor(3 ln d)).
    pub lambda: usize,
    /// Restarts with doubled population (IPOP).
    pub restarts: usize,
}

impl Default for CmaOptions {
    fn default() -> Self {
        Self {
            sigma_frac: 0.3,
            lambda: 0,
            restarts: 0,
        }
    }
}

/// CMA-ES (Hansen) with full covariance, rank-μ update, cumulative step-size
/// adaptation and IPOP restarts.  Constraints via Deb's rules for ranking.
pub fn cma_es(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    opts: &CmaOptions,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let n = d as f64;
    let bounds = p.bounds();
    let c = Counted::new(p);
    let mut rng = seeded(budget.seed);
    let mut history = vec![];
    let mut global_best: Option<(Vec<f64>, Evaluation)> = None;
    let mut iterations = 0usize;
    let mut converged = false;
    let base_lambda = if opts.lambda == 0 {
        4 + (3.0 * n.ln()).floor() as usize
    } else {
        opts.lambda
    };
    let width: Vec<f64> = bounds
        .iter()
        .map(|(lo, hi)| if hi > lo { hi - lo } else { 1.0 })
        .collect();
    for restart in 0..=opts.restarts {
        let lambda = base_lambda * (1 << restart.min(10));
        let mu = lambda / 2;
        let mut weights: Vec<f64> = (0..mu)
            .map(|i| ((mu as f64) + 0.5).ln() - ((i + 1) as f64).ln())
            .collect();
        let wsum: f64 = weights.iter().sum();
        for w in &mut weights {
            *w /= wsum;
        }
        let mueff = 1.0 / weights.iter().map(|w| w * w).sum::<f64>();
        let cc = (4.0 + mueff / n) / (n + 4.0 + 2.0 * mueff / n);
        let cs = (mueff + 2.0) / (n + mueff + 5.0);
        let c1 = 2.0 / ((n + 1.3).powi(2) + mueff);
        let cmu = (1.0 - c1).min(2.0 * (mueff - 2.0 + 1.0 / mueff) / ((n + 2.0).powi(2) + mueff));
        let damps = 1.0 + 2.0 * (0.0f64).max(((mueff - 1.0) / (n + 1.0)).sqrt() - 1.0) + cs;
        let chi_n = n.sqrt() * (1.0 - 1.0 / (4.0 * n) + 1.0 / (21.0 * n * n));

        let mut mean: DVector<f64> = match (x0, restart) {
            (Some(x), 0) => DVector::from_column_slice(x),
            _ => DVector::from_iterator(d, random_point(&bounds, &mut rng)),
        };
        let mut sigma = opts.sigma_frac * width.iter().cloned().fold(0.0, f64::max);
        let mut cov = DMatrix::<f64>::identity(d, d);
        // scale covariance by box width per axis so anisotropic boxes are handled
        for i in 0..d {
            let s = width[i] / width.iter().cloned().fold(0.0, f64::max);
            cov[(i, i)] = s * s;
        }
        let mut pc = DVector::<f64>::zeros(d);
        let mut ps = DVector::<f64>::zeros(d);
        let mut eigen_b = DMatrix::<f64>::identity(d, d);
        let mut eigen_d = DVector::<f64>::from_element(d, 1.0);
        let mut eigeneval = 0usize;
        let mut last_best = f64::INFINITY;
        let mut stall = 0usize;

        while c.count() + lambda <= budget.max_evaluations && iterations < budget.max_iterations {
            ctl.check()?;
            iterations += 1;
            // eigendecomposition (lazy)
            if iterations == 1
                || c.count() - eigeneval > (lambda as f64 / (c1 + cmu) / n / 10.0) as usize
            {
                eigeneval = c.count();
                let sym = 0.5 * (&cov + cov.transpose());
                let e = SymmetricEigen::new(sym);
                eigen_b = e.eigenvectors;
                eigen_d = e.eigenvalues.map(|v| v.max(1e-20).sqrt());
            }
            let mut arz: Vec<DVector<f64>> = Vec::with_capacity(lambda);
            let mut arx: Vec<Vec<f64>> = Vec::with_capacity(lambda);
            let mut evals: Vec<Evaluation> = Vec::with_capacity(lambda);
            for _ in 0..lambda {
                let z = DVector::from_iterator(d, (0..d).map(|_| StandardNormal.sample(&mut rng)));
                let y = &eigen_b * eigen_d.component_mul(&z);
                let mut x: Vec<f64> = (&mean + sigma * &y).as_slice().to_vec();
                clamp_to_bounds(&mut x, &bounds);
                arz.push(z);
                evals.push(c.eval(&x)?);
                arx.push(x);
            }
            let mut idx: Vec<usize> = (0..lambda).collect();
            idx.sort_by(|&a, &b| deb_compare(&evals[a], &evals[b]));
            let old_mean = mean.clone();
            mean = DVector::zeros(d);
            for (k, &i) in idx[..mu].iter().enumerate() {
                mean += weights[k] * DVector::from_column_slice(&arx[i]);
            }
            let y_w = (&mean - &old_mean) / sigma;
            // C^{-1/2} y_w = B D^{-1} B^T y_w
            let inv_sqrt_c_y = &eigen_b * (eigen_b.transpose() * &y_w).component_div(&eigen_d);
            ps = (1.0 - cs) * &ps + (cs * (2.0 - cs) * mueff).sqrt() * inv_sqrt_c_y;
            let ps_norm = ps.norm();
            let hsig = if ps_norm / (1.0 - (1.0 - cs).powi(2 * iterations as i32)).sqrt() / chi_n
                < 1.4 + 2.0 / (n + 1.0)
            {
                1.0
            } else {
                0.0
            };
            pc = (1.0 - cc) * &pc + hsig * (cc * (2.0 - cc) * mueff).sqrt() * &y_w;
            let mut rank_mu = DMatrix::<f64>::zeros(d, d);
            for (k, &i) in idx[..mu].iter().enumerate() {
                let yi = (DVector::from_column_slice(&arx[i]) - &old_mean) / sigma;
                rank_mu += weights[k] * (&yi * yi.transpose());
            }
            cov = (1.0 - c1 - cmu) * &cov
                + c1 * (&pc * pc.transpose() + (1.0 - hsig) * cc * (2.0 - cc) * &cov)
                + cmu * rank_mu;
            sigma *= ((cs / damps) * (ps_norm / chi_n - 1.0)).exp();

            let bi = idx[0];
            let bf = scalarize(&evals[bi], 1e6);
            history.push(HistoryPoint {
                evaluations: c.count(),
                best_f: bf,
            });
            match &global_best {
                Some((_, ge)) if deb_compare(ge, &evals[bi]) != Ordering::Greater => {}
                _ => global_best = Some((arx[bi].clone(), evals[bi].clone())),
            }
            if (last_best - bf).abs() < budget.f_tol {
                stall += 1;
            } else {
                stall = 0;
            }
            last_best = bf;
            let max_std = sigma * eigen_d.max();
            if max_std < budget.x_tol || stall > 10 + 30 * d / lambda.max(1) || !sigma.is_finite() {
                converged = max_std < budget.x_tol;
                break;
            }
        }
    }
    let (best_x, best_eval) = global_best
        .ok_or_else(|| SimorphError::Numerical("CMA-ES produced no evaluation".into()))?;
    Ok(OptimizationResult {
        method: Method::CmaEs,
        best_f: scalarize(&best_eval, 1e6),
        best_x,
        best_eval,
        evaluations: c.count(),
        iterations,
        converged,
        history,
        message: if converged {
            "step size below tolerance".into()
        } else {
            "budget exhausted or stalled".into()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::*;

    #[test]
    fn de_rastrigin_5d_finds_global() {
        let p = rastrigin_problem(5);
        let r = differential_evolution(
            &p,
            &DeOptions {
                restarts: 2,
                ..Default::default()
            },
            &Budget::new(60_000).iterations(100_000).seed(3),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-3, "f={}", r.best_f);
    }

    #[test]
    fn de_g06_constrained() {
        let p = g06_problem();
        let r = differential_evolution(
            &p,
            &DeOptions {
                population: 40,
                ..Default::default()
            },
            &Budget::new(30_000)
                .iterations(100_000)
                .seed(1)
                .tolerance(1e-12, 1e-9),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_eval.is_feasible());
        assert!(
            (r.best_eval.objectives[0] - G06_OPTIMUM).abs() < 0.5,
            "f={}",
            r.best_eval.objectives[0]
        );
    }

    #[test]
    fn cma_rosenbrock_10d() {
        // From the origin CMA-ES can converge to the known local minimum of the
        // 10-D Rosenbrock function (f ≈ 3.98); IPOP restarts must escape it.
        let p = rosenbrock_problem(10);
        let r = cma_es(
            &p,
            Some(&[0.0; 10]),
            &CmaOptions {
                restarts: 3,
                ..Default::default()
            },
            &Budget::new(100_000)
                .iterations(100_000)
                .seed(5)
                .tolerance(1e-14, 1e-9),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(
            r.best_f < 1e-6,
            "f={} it={} ev={}",
            r.best_f,
            r.iterations,
            r.evaluations
        );
    }

    #[test]
    fn cma_rastrigin_with_restarts() {
        let p = rastrigin_problem(3);
        let r = cma_es(
            &p,
            None,
            &CmaOptions {
                restarts: 4,
                ..Default::default()
            },
            &Budget::new(60_000)
                .iterations(100_000)
                .seed(2)
                .tolerance(1e-12, 1e-8),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-3, "f={}", r.best_f);
    }

    #[test]
    fn cma_g06_constrained() {
        let p = g06_problem();
        let r = cma_es(
            &p,
            None,
            &CmaOptions {
                restarts: 3,
                ..Default::default()
            },
            &Budget::new(40_000)
                .iterations(100_000)
                .seed(4)
                .tolerance(1e-12, 1e-9),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_eval.is_feasible());
        assert!(
            (r.best_eval.objectives[0] - G06_OPTIMUM).abs() < 1.0,
            "f={}",
            r.best_eval.objectives[0]
        );
    }
}
