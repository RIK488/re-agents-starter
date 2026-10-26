//! Gradient descent with Armijo backtracking and L-BFGS.

use crate::problem::*;
use simorph_core::{ExecutionControl, Result};

fn grad(p: &dyn ObjectiveFunction, c: &Counted, x: &[f64], penalty: f64) -> Result<Vec<f64>> {
    if p.n_constraints() == 0 {
        if let Some(g) = p.gradient(x) {
            return Ok(g);
        }
    }
    let h = 1e-6;
    let mut g = vec![0.0; x.len()];
    for i in 0..x.len() {
        let hi = h * x[i].abs().max(1.0);
        let mut xp = x.to_vec();
        let mut xm = x.to_vec();
        xp[i] += hi;
        xm[i] -= hi;
        g[i] = (c.eval_scalar(&xp, penalty)? - c.eval_scalar(&xm, penalty)?) / (2.0 * hi);
    }
    Ok(g)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Backtracking line search satisfying the Armijo condition.  Returns the
/// accepted point and value, or None if no decrease could be found.
fn armijo(
    c: &Counted,
    x: &[f64],
    fx: f64,
    g: &[f64],
    dir: &[f64],
    bounds: &[(f64, f64)],
    penalty: f64,
) -> Result<Option<(Vec<f64>, f64)>> {
    let slope = dot(g, dir);
    if slope >= 0.0 {
        return Ok(None);
    }
    let mut t = 1.0;
    for _ in 0..40 {
        let mut xn: Vec<f64> = x.iter().zip(dir).map(|(xi, di)| xi + t * di).collect();
        clamp_to_bounds(&mut xn, bounds);
        let fn_ = c.eval_scalar(&xn, penalty)?;
        if fn_ <= fx + 1e-4 * t * slope {
            return Ok(Some((xn, fn_)));
        }
        t *= 0.5;
    }
    Ok(None)
}

fn run(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    budget: &Budget,
    ctl: &ExecutionControl,
    lbfgs_m: usize,
) -> Result<OptimizationResult> {
    let d = check_dim(p)?;
    let bounds = p.bounds();
    let c = Counted::new(p);
    let penalty = 1e4;
    let mut x: Vec<f64> = match x0 {
        Some(x) => x.to_vec(),
        None => bounds.iter().map(|(lo, hi)| 0.5 * (lo + hi)).collect(),
    };
    clamp_to_bounds(&mut x, &bounds);
    let mut fx = c.eval_scalar(&x, penalty)?;
    let mut g = grad(p, &c, &x, penalty)?;
    let mut s_hist: Vec<Vec<f64>> = vec![];
    let mut y_hist: Vec<Vec<f64>> = vec![];
    let mut history = vec![HistoryPoint {
        evaluations: c.count(),
        best_f: fx,
    }];
    let mut iter = 0;
    let mut converged = false;
    let mut message = String::from("budget exhausted");
    while iter < budget.max_iterations && c.count() < budget.max_evaluations {
        ctl.check()?;
        iter += 1;
        let gnorm = dot(&g, &g).sqrt();
        if gnorm < 1e-8 {
            converged = true;
            message = "gradient norm below tolerance".into();
            break;
        }
        // direction
        let mut dir: Vec<f64> = if lbfgs_m == 0 || s_hist.is_empty() {
            g.iter().map(|v| -v).collect()
        } else {
            // two-loop recursion
            let k = s_hist.len();
            let mut q = g.clone();
            let mut alpha = vec![0.0; k];
            let mut rho = vec![0.0; k];
            for i in (0..k).rev() {
                rho[i] = 1.0 / dot(&y_hist[i], &s_hist[i]);
                alpha[i] = rho[i] * dot(&s_hist[i], &q);
                for j in 0..d {
                    q[j] -= alpha[i] * y_hist[i][j];
                }
            }
            let gamma = dot(&s_hist[k - 1], &y_hist[k - 1]) / dot(&y_hist[k - 1], &y_hist[k - 1]);
            let mut r: Vec<f64> = q.iter().map(|v| gamma * v).collect();
            for i in 0..k {
                let beta = rho[i] * dot(&y_hist[i], &r);
                for j in 0..d {
                    r[j] += s_hist[i][j] * (alpha[i] - beta);
                }
            }
            r.iter().map(|v| -v).collect()
        };
        if dot(&dir, &g) >= 0.0 {
            dir = g.iter().map(|v| -v).collect();
            s_hist.clear();
            y_hist.clear();
        }
        let step = match armijo(&c, &x, fx, &g, &dir, &bounds, penalty)? {
            Some(s) => s,
            None => {
                if lbfgs_m > 0 && !s_hist.is_empty() {
                    s_hist.clear();
                    y_hist.clear();
                    continue;
                }
                converged = true;
                message = "no descent direction found (stationary or at bound)".into();
                break;
            }
        };
        let (xn, fnew) = step;
        let gn = grad(p, &c, &xn, penalty)?;
        let s: Vec<f64> = xn.iter().zip(&x).map(|(a, b)| a - b).collect();
        let y: Vec<f64> = gn.iter().zip(&g).map(|(a, b)| a - b).collect();
        if lbfgs_m > 0 && dot(&s, &y) > 1e-12 {
            s_hist.push(s.clone());
            y_hist.push(y);
            if s_hist.len() > lbfgs_m {
                s_hist.remove(0);
                y_hist.remove(0);
            }
        }
        let df = (fx - fnew).abs();
        let dx = dot(&s, &s).sqrt();
        x = xn;
        fx = fnew;
        g = gn;
        history.push(HistoryPoint {
            evaluations: c.count(),
            best_f: fx,
        });
        if df < budget.f_tol && dx < budget.x_tol {
            converged = true;
            message = "step and improvement below tolerance".into();
            break;
        }
    }
    let best_eval = p.evaluate(&x)?;
    Ok(OptimizationResult {
        method: if lbfgs_m > 0 {
            Method::Lbfgs
        } else {
            Method::GradientDescent
        },
        best_x: x,
        best_f: fx,
        best_eval,
        evaluations: c.count(),
        iterations: iter,
        converged,
        history,
        message,
    })
}

/// Steepest descent with Armijo line search (numerical gradient unless the
/// problem supplies one).
pub fn gradient_descent(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    run(p, x0, budget, ctl, 0)
}

/// Limited-memory BFGS (two-loop recursion, memory `m`).
pub fn lbfgs(
    p: &dyn ObjectiveFunction,
    x0: Option<&[f64]>,
    m: usize,
    budget: &Budget,
    ctl: &ExecutionControl,
) -> Result<OptimizationResult> {
    run(p, x0, budget, ctl, m.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::*;

    #[test]
    fn lbfgs_rosenbrock_10d() {
        let p = rosenbrock_problem(10);
        let r = lbfgs(
            &p,
            Some(&[-1.2; 10]),
            10,
            &Budget::new(200_000)
                .iterations(5000)
                .tolerance(1e-16, 1e-14),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(
            r.best_f < 1e-6,
            "f={} iters={} evals={}",
            r.best_f,
            r.iterations,
            r.evaluations
        );
    }

    #[test]
    fn gradient_descent_sphere() {
        let p = sphere_problem(5);
        let r = gradient_descent(
            &p,
            Some(&[2.0; 5]),
            &Budget::new(10000),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-10);
        assert!(r.converged);
    }

    #[test]
    fn analytic_gradient_is_used() {
        struct Quad;
        impl ObjectiveFunction for Quad {
            fn dimension(&self) -> usize {
                2
            }
            fn bounds(&self) -> Vec<(f64, f64)> {
                vec![(-10.0, 10.0); 2]
            }
            fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
                Ok(Evaluation::scalar(
                    (x[0] - 1.0).powi(2) + 10.0 * (x[1] + 2.0).powi(2),
                ))
            }
            fn gradient(&self, x: &[f64]) -> Option<Vec<f64>> {
                Some(vec![2.0 * (x[0] - 1.0), 20.0 * (x[1] + 2.0)])
            }
        }
        let r = lbfgs(
            &Quad,
            Some(&[5.0, 5.0]),
            5,
            &Budget::new(1000),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.best_f < 1e-12);
        assert!(r.evaluations < 100, "evals {}", r.evaluations);
    }
}
