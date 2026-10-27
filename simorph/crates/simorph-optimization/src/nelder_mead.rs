//! Nelder–Mead downhill simplex.

use crate::problem::*;
use simorph_core::{ExecutionControl, Result};

/// Nelder–Mead with standard coefficients (α=1, γ=2, ρ=0.5, σ=0.5) and
/// bound clamping.  Starts from `x0` (or the box centre) with initial
/// simplex size 5 % of the box width.
pub fn nelder_mead(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    crate::problem::guarded(p, budget, Method::NelderMead, |g| {
        nelder_mead_impl(g, x0, budget, ctl)
    })
}

fn nelder_mead_impl(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let bounds = p.bounds();
    let c = Counted::new(p);
    let penalty = 1e6;
    let start: Vec<f64> = match x0 {
        Some(x) => x.to_vec(),
        None => bounds.iter().map(|(lo, hi)| 0.5 * (lo + hi)).collect(),
    };
    let mut simplex: Vec<Vec<f64>> = vec![start.clone()];
    for i in 0..d {
        let mut v = start.clone();
        let w = (bounds[i].1 - bounds[i].0) * 0.05;
        v[i] += if w > 0.0 {
            w
        } else {
            0.05 * v[i].abs().max(1.0)
        };
        clamp_to_bounds(&mut v, &bounds);
        simplex.push(v);
    }
    let mut fs: Vec<f64> = simplex
        .iter()
        .map(|x| c.eval_scalar(x, penalty))
        .collect::<Result<_>>()?;
    let mut history = vec![];
    let mut iter = 0;
    let mut converged = false;
    while iter < budget.max_iterations && c.count() + 2 < budget.max_evaluations {
        ctl.check()?;
        iter += 1;
        // sort
        let mut idx: Vec<usize> = (0..=d).collect();
        idx.sort_by(|&a, &b| fs[a].total_cmp(&fs[b]));
        simplex = idx.iter().map(|&i| simplex[i].clone()).collect();
        fs = idx.iter().map(|&i| fs[i]).collect();
        history.push(HistoryPoint {
            evaluations: c.count(),
            best_f: fs[0],
        });
        let fspread = fs[d] - fs[0];
        let xspread = (1..=d)
            .map(|i| {
                (0..d)
                    .map(|j| (simplex[i][j] - simplex[0][j]).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        if fspread.abs() < budget.f_tol && xspread < budget.x_tol {
            converged = true;
            break;
        }
        let centroid: Vec<f64> = (0..d)
            .map(|j| simplex[..d].iter().map(|x| x[j]).sum::<f64>() / d as f64)
            .collect();
        let reflect = |t: f64| -> Vec<f64> {
            let mut v: Vec<f64> = (0..d)
                .map(|j| centroid[j] + t * (simplex[d][j] - centroid[j]))
                .collect();
            clamp_to_bounds(&mut v, &bounds);
            v
        };
        let xr = reflect(-1.0);
        let fr = c.eval_scalar(&xr, penalty)?;
        if fr < fs[0] {
            let xe = reflect(-2.0);
            let fe = c.eval_scalar(&xe, penalty)?;
            if fe < fr {
                simplex[d] = xe;
                fs[d] = fe;
            } else {
                simplex[d] = xr;
                fs[d] = fr;
            }
        } else if fr < fs[d - 1] {
            simplex[d] = xr;
            fs[d] = fr;
        } else {
            let (xc, fc) = if fr < fs[d] {
                let xc = reflect(-0.5);
                let fc = c.eval_scalar(&xc, penalty)?;
                (xc, fc)
            } else {
                let xc = reflect(0.5);
                let fc = c.eval_scalar(&xc, penalty)?;
                (xc, fc)
            };
            if fc < fs[d].min(fr) {
                simplex[d] = xc;
                fs[d] = fc;
            } else {
                // shrink
                for i in 1..=d {
                    for j in 0..d {
                        simplex[i][j] = simplex[0][j] + 0.5 * (simplex[i][j] - simplex[0][j]);
                    }
                    fs[i] = c.eval_scalar(&simplex[i], penalty)?;
                }
            }
        }
    }
    let bi = (0..=d).min_by(|&a, &b| fs[a].total_cmp(&fs[b])).unwrap();
    let best_eval = p.evaluate(&simplex[bi])?;
    Ok(OptimizationResult {
        method: Method::NelderMead,
        best_x: simplex[bi].clone(),
        best_f: fs[bi],
        best_eval,
        evaluations: c.count(),
        iterations: iter,
        converged,
        history,
        message: if converged {
            "simplex collapsed below tolerance".into()
        } else {
            "budget exhausted".into()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::*;

    #[test]
    fn rosenbrock_2d() {
        let p = rosenbrock_problem(2);
        let r = nelder_mead(
            &p,
            Some(&[-1.2, 1.0]),
            &Budget::new(5000).tolerance(1e-14, 1e-10),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-8, "f={}", r.best_f);
        assert!((r.best_x[0] - 1.0).abs() < 1e-4 && (r.best_x[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn sphere_10d() {
        let p = sphere_problem(10);
        let r = nelder_mead(
            &p,
            Some(&[1.0; 10]),
            &Budget::new(20000).iterations(20000).tolerance(1e-16, 1e-12),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-6, "f={}", r.best_f);
    }
}
