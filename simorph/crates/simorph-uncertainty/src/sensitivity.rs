//! Global and local sensitivity analysis.

use crate::propagation::evaluate_samples;
use crate::rng::seeded;
use crate::sampling::{map_to_inputs, unit_design, Design, InputSpec};
use crate::stats::{mean, variance};
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};

/// Sobol indices for one input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SobolIndex {
    pub name: String,
    pub first_order: f64,
    pub total_order: f64,
    /// Bootstrap 95% CI of the first-order index.
    pub first_order_ci95: (f64, f64),
    pub total_order_ci95: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SobolResult {
    pub indices: Vec<SobolIndex>,
    pub output_variance: f64,
    pub output_mean: f64,
    /// Number of base samples N; total evaluations = N (d + 2).
    pub n_base: usize,
    pub evaluations: usize,
    /// Sampling design actually used ("sobol-scrambled" or the fallback).
    #[serde(default)]
    pub design: String,
    /// Conditions that weaken the estimates (fallback design, N not a power
    /// of two for Sobol, near-zero output variance …).
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Smallest accepted base sample size for `d` inputs.
pub fn sobol_min_base_samples(d: usize) -> usize {
    4 * (d + 2)
}

fn sobol_estimators(fa: &[f64], fb: &[f64], fab: &[f64], idx: &[usize]) -> (f64, f64) {
    // Saltelli 2010 first-order and Jansen total-order estimators.
    let n = idx.len() as f64;
    let mut all = Vec::with_capacity(2 * idx.len());
    let mut s1 = 0.0;
    let mut st = 0.0;
    for &i in idx {
        all.push(fa[i]);
        all.push(fb[i]);
        s1 += fb[i] * (fab[i] - fa[i]);
        st += (fa[i] - fab[i]).powi(2);
    }
    let var = variance(&all);
    if var <= 0.0 {
        return (0.0, 0.0);
    }
    (s1 / n / var, st / (2.0 * n) / var)
}

/// Sobol first- and total-order indices by the Saltelli scheme, with
/// bootstrap confidence intervals.  `f` returns a scalar output (use
/// `output_index` to select a component from a vector-valued model).
pub fn sobol_indices<F>(
    inputs: &[InputSpec],
    n: usize,
    seed: u64,
    output_index: usize,
    f: F,
    ctl: &ExecutionControl,
) -> Result<SobolResult>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    let d = inputs.len();
    if d == 0 {
        return Err(SimorphError::InvalidInput("Sobol analysis needs at least one input".into()));
    }
    // audit: tiny N gave meaningless indices without any warning
    if n < sobol_min_base_samples(d) {
        return Err(SimorphError::InvalidInput(format!(
            "Sobol analysis with {d} inputs needs N ≥ {} base samples (got {n})",
            sobol_min_base_samples(d)
        )));
    }
    let mut warnings = vec![];
    // A and B from one 2d-dimensional Sobol design (standard practice).
    let design = if 2 * d <= crate::sampling::SOBOL_MAX_DIM {
        if !n.is_power_of_two() {
            warnings.push(format!(
                "N = {n} is not a power of two: the Sobol net's balance properties are lost"
            ));
        }
        Design::Sobol
    } else {
        warnings.push(format!(
            "2d = {} exceeds the {} Sobol dimensions available: fell back to Latin hypercube \
             (slower convergence)",
            2 * d,
            crate::sampling::SOBOL_MAX_DIM
        ));
        Design::LatinHypercube
    };
    let unit = unit_design(design, n, 2 * d, seed);
    let a_unit: Vec<Vec<f64>> = unit.iter().map(|p| p[..d].to_vec()).collect();
    let b_unit: Vec<Vec<f64>> = unit.iter().map(|p| p[d..].to_vec()).collect();
    let a = map_to_inputs(&a_unit, inputs)?;
    let b = map_to_inputs(&b_unit, inputs)?;
    let mut all: Vec<Vec<f64>> = Vec::with_capacity(n * (d + 2));
    all.extend(a.iter().cloned());
    all.extend(b.iter().cloned());
    for i in 0..d {
        for k in 0..n {
            let mut x = a[k].clone();
            x[i] = b[k][i];
            all.push(x);
        }
    }
    let (_, out, msgs) = evaluate_samples(&all, &f, ctl)?;
    if out.len() != all.len() {
        return Err(SimorphError::Numerical(format!(
            "{} of {} Sobol evaluations failed: {}",
            msgs.len(),
            all.len(),
            msgs.first().cloned().unwrap_or_default()
        )));
    }
    let y: Vec<f64> = out
        .iter()
        .map(|o| {
            o.get(output_index)
                .copied()
                .ok_or_else(|| SimorphError::InvalidInput("output index out of range".into()))
        })
        .collect::<Result<_>>()?;
    let fa = &y[..n];
    let fb = &y[n..2 * n];
    let full: Vec<usize> = (0..n).collect();
    let mut rng = seeded(seed.wrapping_add(1));
    let boots = 200;
    let mut indices = vec![];
    let ab_all: Vec<f64> = fa.iter().chain(fb.iter()).copied().collect();
    for i in 0..d {
        let fab = &y[(2 + i) * n..(3 + i) * n];
        let (s1, st) = sobol_estimators(fa, fb, fab, &full);
        let mut b1 = Vec::with_capacity(boots);
        let mut bt = Vec::with_capacity(boots);
        for _ in 0..boots {
            let idx: Vec<usize> = (0..n).map(|_| rng.gen_range(0..n)).collect();
            let (x, t) = sobol_estimators(fa, fb, fab, &idx);
            b1.push(x);
            bt.push(t);
        }
        b1.sort_by(|a, b| a.total_cmp(b));
        bt.sort_by(|a, b| a.total_cmp(b));
        indices.push(SobolIndex {
            name: inputs[i].name.clone(),
            first_order: s1,
            total_order: st,
            first_order_ci95: (
                crate::stats::quantile(&b1, 0.025),
                crate::stats::quantile(&b1, 0.975),
            ),
            total_order_ci95: (
                crate::stats::quantile(&bt, 0.025),
                crate::stats::quantile(&bt, 0.975),
            ),
        });
    }
    let output_variance = variance(&ab_all);
    if !(output_variance > 1e-300) {
        warnings.push("output variance is zero: indices are undefined and reported as 0".into());
    }
    Ok(SobolResult {
        indices,
        output_variance,
        output_mean: mean(&ab_all),
        n_base: n,
        evaluations: all.len(),
        design: match design {
            Design::Sobol => "sobol-scrambled".into(),
            _ => "latin-hypercube".into(),
        },
        warnings,
    })
}

/// Morris elementary effects for one input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorrisEffect {
    pub name: String,
    /// Mean of absolute elementary effects (overall influence).
    pub mu_star: f64,
    /// Mean of signed elementary effects.
    pub mu: f64,
    /// Standard deviation (non-linearity / interactions).
    pub sigma: f64,
}

/// Morris screening with `r` trajectories and `levels` grid levels.
/// Inputs are sampled on the unit cube and mapped by inverse CDF, so the
/// effects are expressed per unit of the (0,1) probability scale.
pub fn morris_screening<F>(
    inputs: &[InputSpec],
    r: usize,
    levels: usize,
    seed: u64,
    output_index: usize,
    f: F,
    ctl: &ExecutionControl,
) -> Result<Vec<MorrisEffect>>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    let d = inputs.len();
    if r == 0 || d == 0 {
        // audit: r = 0 returned NaN effects as if they were results
        return Err(SimorphError::InvalidInput(format!(
            "Morris screening needs r ≥ 1 trajectories and ≥ 1 input (r = {r}, d = {d})"
        )));
    }
    let p = levels.max(2);
    let delta = p as f64 / (2.0 * (p as f64 - 1.0));
    let mut rng = seeded(seed);
    // Build trajectories: base point on the grid with coordinates in {0, 1/(p-1), ..., 1-delta}
    let mut points: Vec<Vec<f64>> = Vec::with_capacity(r * (d + 1));
    let mut orders: Vec<Vec<usize>> = Vec::with_capacity(r);
    let mut signs: Vec<Vec<f64>> = Vec::with_capacity(r);
    for _ in 0..r {
        let mut x: Vec<f64> = (0..d)
            .map(|_| {
                let k = rng.gen_range(0..(p / 2));
                k as f64 / (p as f64 - 1.0)
            })
            .collect();
        let mut order: Vec<usize> = (0..d).collect();
        use rand::seq::SliceRandom;
        order.shuffle(&mut rng);
        let mut sgn = vec![0.0; d];
        points.push(x.clone());
        for &j in &order {
            // move up if it stays inside [0,1], else down
            let s = if x[j] + delta <= 1.0 + 1e-12 {
                1.0
            } else {
                -1.0
            };
            x[j] += s * delta;
            sgn[j] = s;
            points.push(x.clone());
        }
        orders.push(order);
        signs.push(sgn);
    }
    let mapped = map_to_inputs(&points, inputs)?;
    let (_, out, msgs) = evaluate_samples(&mapped, &f, ctl)?;
    if out.len() != mapped.len() {
        return Err(SimorphError::Numerical(format!(
            "Morris evaluations failed: {:?}",
            msgs.first()
        )));
    }
    let mut effects: Vec<Vec<f64>> = vec![vec![]; d];
    for t in 0..r {
        let base = t * (d + 1);
        for (step, &j) in orders[t].iter().enumerate() {
            let (Some(&y0), Some(&y1)) = (
                out[base + step].get(output_index),
                out[base + step + 1].get(output_index),
            ) else {
                return Err(SimorphError::InvalidInput("output index out of range".into()));
            };
            effects[j].push((y1 - y0) / (signs[t][j] * delta));
        }
    }
    Ok(effects
        .iter()
        .enumerate()
        .map(|(j, e)| {
            let abs: Vec<f64> = e.iter().map(|v| v.abs()).collect();
            MorrisEffect {
                name: inputs[j].name.clone(),
                mu_star: mean(&abs),
                mu: mean(e),
                sigma: variance(e).sqrt(),
            }
        })
        .collect())
}

/// Local sensitivity by central finite differences at `x0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSensitivity {
    pub name: String,
    /// dy/dx_i
    pub gradient: f64,
    /// (x_i / y) dy/dx_i  (relative / elasticity); NaN if y == 0.
    pub normalised: f64,
}

pub fn local_sensitivity<F>(
    names: &[String],
    x0: &[f64],
    rel_step: f64,
    output_index: usize,
    f: F,
) -> Result<Vec<LocalSensitivity>>
where
    F: Fn(&[f64]) -> Result<Vec<f64>>,
{
    let y0 = f(x0)?[output_index];
    let mut out = vec![];
    for i in 0..x0.len() {
        let h = rel_step * x0[i].abs().max(1.0);
        let mut xp = x0.to_vec();
        let mut xm = x0.to_vec();
        xp[i] += h;
        xm[i] -= h;
        let g = (f(&xp)?[output_index] - f(&xm)?[output_index]) / (2.0 * h);
        out.push(LocalSensitivity {
            name: names.get(i).cloned().unwrap_or_else(|| format!("x{i}")),
            gradient: g,
            normalised: if y0 != 0.0 { g * x0[i] / y0 } else { f64::NAN },
        });
    }
    Ok(out)
}

/// Numerical gradient (central differences) of a scalar function.
pub fn numerical_gradient<F>(x: &[f64], f: F, h: f64) -> Result<Vec<f64>>
where
    F: Fn(&[f64]) -> Result<f64>,
{
    let mut g = vec![0.0; x.len()];
    for i in 0..x.len() {
        let hi = h * x[i].abs().max(1.0);
        let mut xp = x.to_vec();
        let mut xm = x.to_vec();
        xp[i] += hi;
        xm[i] -= hi;
        g[i] = (f(&xp)? - f(&xm)?) / (2.0 * hi);
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::Uncertainty;
    use std::f64::consts::PI;

    fn ishigami(x: &[f64]) -> Result<Vec<f64>> {
        let (a, b) = (7.0, 0.1);
        Ok(vec![
            x[0].sin() + a * x[1].sin().powi(2) + b * x[2].powi(4) * x[0].sin(),
        ])
    }

    fn ishigami_inputs() -> Vec<InputSpec> {
        (0..3)
            .map(|i| {
                InputSpec::new(
                    &format!("x{}", i + 1),
                    0.0,
                    Uncertainty::Uniform { low: -PI, high: PI },
                )
            })
            .collect()
    }

    #[test]
    fn ishigami_sobol_indices_match_analytic() {
        let r = sobol_indices(
            &ishigami_inputs(),
            4096,
            3,
            0,
            ishigami,
            &ExecutionControl::default(),
        )
        .unwrap();
        let expect_s = [0.3139, 0.4424, 0.0];
        let expect_t = [0.5576, 0.4424, 0.2437];
        for (i, idx) in r.indices.iter().enumerate() {
            assert!(
                (idx.first_order - expect_s[i]).abs() < 0.05,
                "S{} = {}",
                i + 1,
                idx.first_order
            );
            assert!(
                (idx.total_order - expect_t[i]).abs() < 0.05,
                "ST{} = {}",
                i + 1,
                idx.total_order
            );
            assert!(
                idx.first_order_ci95.0 <= idx.first_order
                    && idx.first_order <= idx.first_order_ci95.1
            );
        }
        // analytic variance of Ishigami: a^2/8 + b pi^4/5 + b^2 pi^8/18 + 1/2 = 13.8445
        assert!(
            (r.output_variance - 13.8445).abs() < 0.4,
            "var {}",
            r.output_variance
        );
        assert_eq!(r.evaluations, 4096 * 5);
    }

    #[test]
    fn morris_identifies_inactive_input() {
        let inputs = vec![
            InputSpec::uniform("a", 0.0, 1.0),
            InputSpec::uniform("b", 0.0, 1.0),
            InputSpec::uniform("inactive", 0.0, 1.0),
        ];
        let eff = morris_screening(
            &inputs,
            20,
            4,
            5,
            0,
            |x| Ok(vec![3.0 * x[0] + x[1] * x[1]]),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!((eff[0].mu_star - 3.0).abs() < 1e-9);
        assert!(eff[2].mu_star.abs() < 1e-12);
        assert!(eff[1].mu_star > 0.0 && eff[1].sigma > 0.0);
        assert!(eff[0].sigma < 1e-9);
    }

    #[test]
    fn local_sensitivity_of_product() {
        let names = vec!["a".to_string(), "b".to_string()];
        let s = local_sensitivity(&names, &[2.0, 3.0], 1e-6, 0, |x| Ok(vec![x[0] * x[1]])).unwrap();
        assert!((s[0].gradient - 3.0).abs() < 1e-6);
        assert!((s[0].normalised - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sobol_is_deterministic() {
        let a = sobol_indices(
            &ishigami_inputs(),
            256,
            1,
            0,
            ishigami,
            &ExecutionControl::default(),
        )
        .unwrap();
        let b = sobol_indices(
            &ishigami_inputs(),
            256,
            1,
            0,
            ishigami,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(a, b);
    }
}
