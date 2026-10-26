//! Small dense linear algebra used by the implicit solvers.

use simorph_core::{Result, SimorphError};

/// Solve `A x = b` in place for a row-major `n×n` matrix using LU with
/// partial pivoting.  `a` is destroyed; `b` receives the solution.
pub fn solve_dense(a: &mut [f64], b: &mut [f64]) -> Result<()> {
    let n = b.len();
    if a.len() != n * n {
        return Err(SimorphError::InvalidInput(
            "matrix/vector size mismatch".into(),
        ));
    }
    for k in 0..n {
        // pivot
        let mut p = k;
        let mut maxv = a[k * n + k].abs();
        for i in (k + 1)..n {
            let v = a[i * n + k].abs();
            if v > maxv {
                maxv = v;
                p = i;
            }
        }
        if maxv == 0.0 || !maxv.is_finite() {
            return Err(SimorphError::Numerical(
                "singular Jacobian in Newton iteration".into(),
            ));
        }
        if p != k {
            for j in 0..n {
                a.swap(k * n + j, p * n + j);
            }
            b.swap(k, p);
        }
        let pivot = a[k * n + k];
        for i in (k + 1)..n {
            let f = a[i * n + k] / pivot;
            if f == 0.0 {
                continue;
            }
            for j in k..n {
                a[i * n + j] -= f * a[k * n + j];
            }
            b[i] -= f * b[k];
        }
    }
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in (i + 1)..n {
            s -= a[i * n + j] * b[j];
        }
        b[i] = s / a[i * n + i];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_3x3() {
        let mut a = vec![2.0, 1.0, -1.0, -3.0, -1.0, 2.0, -2.0, 1.0, 2.0];
        let mut b = vec![8.0, -11.0, -3.0];
        solve_dense(&mut a, &mut b).unwrap();
        assert!(
            (b[0] - 2.0).abs() < 1e-12 && (b[1] - 3.0).abs() < 1e-12 && (b[2] + 1.0).abs() < 1e-12
        );
    }

    #[test]
    fn singular_is_error() {
        let mut a = vec![1.0, 2.0, 2.0, 4.0];
        let mut b = vec![1.0, 2.0];
        assert!(solve_dense(&mut a, &mut b).is_err());
    }
}
