//! Metrics of a surrogate on the sealed suite and error maps on probe sets.

use crate::suite::{Cell, Sample};
use crate::targets::TargetSystem;
use indexmap::IndexMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_learning::Mlp;

/// Surrogate prediction in physical units.
pub fn predict(model: &Mlp, target: &TargetSystem, x: &[f64]) -> f64 {
    target
        .output_transform
        .inverse(model.predict(&target.features(x))[0])
}

/// Error in transformed output space (log10 for both targets: a relative error).
pub fn point_error(model: &Mlp, target: &TargetSystem, x: &[f64], y: f64) -> f64 {
    model.predict(&target.features(x))[0] - target.output_transform.forward(y).unwrap_or(f64::NAN)
}

fn rmse_of(e: &[f64]) -> f64 {
    (e.iter().map(|v| v * v).sum::<f64>() / e.len().max(1) as f64).sqrt()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuiteEvaluation {
    pub suite_hash: String,
    pub model_hash: String,
    pub n: usize,
    /// Primary metric: RMSE of log10(prediction / truth).
    pub rmse_log10: f64,
    /// Max relative error |pred − truth| / truth.
    pub max_relative_error: f64,
    pub median_relative_error: f64,
    /// Protected metric: RMSE restricted to the protected regime.
    pub protected_regime: String,
    pub protected_rmse_log10: f64,
    pub protected_n: usize,
    /// Protected metric: fraction of grid segments violating the required monotonicity.
    pub monotonicity_violation_rate: f64,
    /// Diagnostic: RMSE by physical regime.
    pub rmse_by_regime: IndexMap<String, (usize, f64)>,
    /// Per-point log10 errors, in suite order (for paired tests).
    pub errors: Vec<f64>,
}

pub fn evaluate_on_suite(
    model: &Mlp,
    target: &TargetSystem,
    points: &[(Vec<f64>, f64)],
    suite_hash: &str,
) -> SuiteEvaluation {
    let errors: Vec<f64> = points
        .iter()
        .map(|(x, y)| point_error(model, target, x, *y))
        .collect();
    let mut rel: Vec<f64> = points
        .iter()
        .map(|(x, y)| (predict(model, target, x) - y).abs() / y.abs().max(1e-300))
        .collect();
    rel.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut prot = vec![];
    let mut by_regime: IndexMap<String, Vec<f64>> = IndexMap::new();
    for ((x, _), e) in points.iter().zip(&errors) {
        let m = target.input_map(x);
        if (target.protected_regime.1)(&m) {
            prot.push(*e);
        }
        by_regime
            .entry((target.regime_label)(&m))
            .or_default()
            .push(*e);
    }
    SuiteEvaluation {
        suite_hash: suite_hash.to_string(),
        model_hash: model.content_hash(),
        n: points.len(),
        rmse_log10: rmse_of(&errors),
        max_relative_error: rel.last().copied().unwrap_or(f64::NAN),
        median_relative_error: rel.get(rel.len() / 2).copied().unwrap_or(f64::NAN),
        protected_regime: target.protected_regime.0.clone(),
        protected_rmse_log10: rmse_of(&prot),
        protected_n: prot.len(),
        monotonicity_violation_rate: monotonicity_violations(model, target),
        rmse_by_regime: by_regime
            .into_iter()
            .map(|(k, v)| (k, (v.len(), rmse_of(&v))))
            .collect(),
        errors,
    }
}

/// Fraction of consecutive grid steps along the monotone input where the
/// surrogate decreases although the physics requires an increase.
pub fn monotonicity_violations(model: &Mlp, target: &TargetSystem) -> f64 {
    let Some((j, increasing)) = target.monotone_in else {
        return 0.0;
    };
    let d = target.inputs.len();
    let lines = 5usize;
    let steps = 25usize;
    let mut total = 0usize;
    let mut bad = 0usize;
    let n_lines = lines.pow((d - 1) as u32);
    for l in 0..n_lines {
        let mut base = vec![0.0; d];
        let mut code = l;
        for (k, dim) in target.inputs.iter().enumerate() {
            if k == j {
                continue;
            }
            let i = code % lines;
            code /= lines;
            let t = dim.t_low() + (dim.t_high() - dim.t_low()) * (i as f64 + 0.5) / lines as f64;
            base[k] = dim.transform.inverse(t);
        }
        let dim = &target.inputs[j];
        let mut prev = None;
        for s in 0..=steps {
            let t = dim.t_low() + (dim.t_high() - dim.t_low()) * s as f64 / steps as f64;
            base[j] = dim.transform.inverse(t);
            let y = model.predict(&target.features(&base))[0];
            if let Some(p) = prev {
                total += 1;
                if (increasing && y < p) || (!increasing && y > p) {
                    bad += 1;
                }
            }
            prev = Some(y);
        }
    }
    bad as f64 / total.max(1) as f64
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellError {
    pub cell: Cell,
    pub n_probe: usize,
    pub n_train: usize,
    pub sse: f64,
    pub rmse: f64,
    /// Share of the total squared error on the probe set.
    pub share: f64,
}

/// Error map: partition the transformed input space into `bins^d` cells,
/// compute the probe-set squared error per cell and the training density.
pub fn error_map(
    model: &Mlp,
    target: &TargetSystem,
    probe: &[Sample],
    train: &[Sample],
    bins: usize,
) -> Vec<CellError> {
    let d = target.inputs.len();
    let cell_of = |x: &[f64]| -> Vec<usize> {
        target
            .features(x)
            .iter()
            .zip(&target.inputs)
            .map(|(f, dim)| {
                (((f - dim.t_low()) / (dim.t_high() - dim.t_low()) * bins as f64).floor() as isize)
                    .clamp(0, bins as isize - 1) as usize
            })
            .collect()
    };
    let mut cells: IndexMap<Vec<usize>, (usize, usize, f64)> = IndexMap::new();
    let n_cells = bins.pow(d as u32);
    for c in 0..n_cells {
        let mut idx = vec![0; d];
        let mut code = c;
        for v in idx.iter_mut() {
            *v = code % bins;
            code /= bins;
        }
        cells.insert(idx, (0, 0, 0.0));
    }
    let mut total = 0.0;
    for s in probe {
        let e = point_error(model, target, &s.inputs, s.output);
        let ent = cells.get_mut(&cell_of(&s.inputs)).expect("cell exists");
        ent.0 += 1;
        ent.2 += e * e;
        total += e * e;
    }
    for s in train {
        cells.get_mut(&cell_of(&s.inputs)).expect("cell exists").1 += 1;
    }
    let mut out: Vec<CellError> = cells
        .into_iter()
        .map(|(idx, (np, nt, sse))| {
            let low = idx
                .iter()
                .zip(&target.inputs)
                .map(|(i, dim)| {
                    dim.t_low() + (dim.t_high() - dim.t_low()) * *i as f64 / bins as f64
                })
                .collect();
            let high = idx
                .iter()
                .zip(&target.inputs)
                .map(|(i, dim)| {
                    dim.t_low() + (dim.t_high() - dim.t_low()) * (*i as f64 + 1.0) / bins as f64
                })
                .collect();
            CellError {
                cell: Cell {
                    index: idx,
                    low,
                    high,
                },
                n_probe: np,
                n_train: nt,
                sse,
                rmse: if np > 0 {
                    (sse / np as f64).sqrt()
                } else {
                    0.0
                },
                share: if total > 0.0 { sse / total } else { 0.0 },
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.sse
            .partial_cmp(&a.sse)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Paired bootstrap of the mean reduction of squared error,
/// `mean(e_old² − e_new²)`; returns (mean, 95 % CI low, 95 % CI high).
pub fn paired_bootstrap(old: &[f64], new: &[f64], resamples: usize, seed: u64) -> (f64, f64, f64) {
    let d: Vec<f64> = old.iter().zip(new).map(|(a, b)| a * a - b * b).collect();
    let n = d.len();
    let mean = d.iter().sum::<f64>() / n.max(1) as f64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut means: Vec<f64> = (0..resamples)
        .map(|_| {
            let mut s = 0.0;
            for _ in 0..n {
                s += d[rng.gen_range(0..n)];
            }
            s / n as f64
        })
        .collect();
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = means[(0.025 * resamples as f64) as usize];
    let hi = means[((0.975 * resamples as f64) as usize).min(resamples - 1)];
    (mean, lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_detects_a_real_shift_and_not_noise() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let old: Vec<f64> = (0..200).map(|_| rng.gen_range(-0.2..0.2)).collect();
        let better: Vec<f64> = old.iter().map(|e| e * 0.5).collect();
        let (_, lo, _) = paired_bootstrap(&old, &better, 2000, 3);
        assert!(lo > 0.0);
        let same: Vec<f64> = old.iter().map(|e| -e).collect(); // identical squared errors
        let (m, lo, hi) = paired_bootstrap(&old, &same, 2000, 3);
        assert!(m.abs() < 1e-15 && lo <= 0.0 && hi >= 0.0);
    }
}
