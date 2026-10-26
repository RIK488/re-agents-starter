//! Quadratic response-surface surrogate with leave-one-out error estimate.

use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};

/// Full quadratic polynomial surrogate: 1, x_i, x_i x_j (i <= j).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuadraticSurrogate {
    pub dim: usize,
    pub coefficients: Vec<f64>,
    /// Leave-one-out root-mean-square prediction error.
    pub loo_rmse: f64,
    /// Coefficient of determination on the training set.
    pub r2: f64,
    pub n_train: usize,
}

fn features(x: &[f64]) -> Vec<f64> {
    let d = x.len();
    let mut f = Vec::with_capacity(1 + d + d * (d + 1) / 2);
    f.push(1.0);
    f.extend_from_slice(x);
    for i in 0..d {
        for j in i..d {
            f.push(x[i] * x[j]);
        }
    }
    f
}

impl QuadraticSurrogate {
    pub fn n_terms(dim: usize) -> usize {
        1 + dim + dim * (dim + 1) / 2
    }

    /// Fit by least squares (SVD).  Requires at least `n_terms(d)` points.
    pub fn fit(xs: &[Vec<f64>], ys: &[f64]) -> Result<Self> {
        let n = xs.len();
        if n == 0 || n != ys.len() {
            return Err(SimorphError::InvalidInput(
                "surrogate: empty or mismatched data".into(),
            ));
        }
        let d = xs[0].len();
        let p = Self::n_terms(d);
        if n < p {
            return Err(SimorphError::InvalidInput(format!(
                "surrogate: need at least {p} points for dimension {d}, got {n}"
            )));
        }
        let mut a = DMatrix::zeros(n, p);
        for (i, x) in xs.iter().enumerate() {
            for (j, v) in features(x).into_iter().enumerate() {
                a[(i, j)] = v;
            }
        }
        let y = DVector::from_column_slice(ys);
        let svd = a.clone().svd(true, true);
        let beta = svd
            .solve(&y, 1e-12)
            .map_err(|e| SimorphError::Numerical(format!("surrogate SVD solve: {e}")))?;
        // Hat matrix diagonal via pseudo-inverse: H = A (A^T A)^-1 A^T
        let ata = a.transpose() * &a;
        let ata_inv = ata
            .try_inverse()
            .ok_or_else(|| SimorphError::Numerical("surrogate: singular normal matrix".into()))?;
        let resid = &y - &a * &beta;
        let mut loo_sq = 0.0;
        for i in 0..n {
            let ai = a.row(i);
            let h = (ai * &ata_inv * ai.transpose())[(0, 0)];
            let denom = (1.0 - h).max(1e-12);
            loo_sq += (resid[i] / denom).powi(2);
        }
        let ymean = y.mean();
        let ss_tot: f64 = y.iter().map(|v| (v - ymean).powi(2)).sum();
        let ss_res: f64 = resid.iter().map(|v| v * v).sum();
        Ok(Self {
            dim: d,
            coefficients: beta.as_slice().to_vec(),
            loo_rmse: (loo_sq / n as f64).sqrt(),
            r2: if ss_tot > 0.0 {
                1.0 - ss_res / ss_tot
            } else {
                1.0
            },
            n_train: n,
        })
    }

    pub fn predict(&self, x: &[f64]) -> f64 {
        features(x)
            .iter()
            .zip(&self.coefficients)
            .map(|(a, b)| a * b)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::latin_hypercube;

    #[test]
    fn recovers_exact_quadratic() {
        let xs = latin_hypercube(30, 2, 3);
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 1.0 + 2.0 * x[0] - x[1] + 0.5 * x[0] * x[1] + 3.0 * x[1] * x[1])
            .collect();
        let s = QuadraticSurrogate::fit(&xs, &ys).unwrap();
        assert!(s.loo_rmse < 1e-9, "loo {}", s.loo_rmse);
        assert!((s.predict(&[0.3, 0.7]) - (1.0 + 0.6 - 0.7 + 0.105 + 1.47)).abs() < 1e-9);
        assert!(s.r2 > 0.999999);
    }

    #[test]
    fn loo_error_reports_model_inadequacy() {
        let xs = latin_hypercube(40, 1, 4);
        let ys: Vec<f64> = xs.iter().map(|x| (10.0 * x[0]).sin()).collect();
        let s = QuadraticSurrogate::fit(&xs, &ys).unwrap();
        assert!(s.loo_rmse > 0.1);
    }
}
