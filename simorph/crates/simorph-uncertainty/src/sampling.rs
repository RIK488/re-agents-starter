//! Sampling from declared uncertainty models and space-filling designs.

use crate::rng::{seeded, Rng};
use rand::seq::SliceRandom;
use rand::Rng as _;
use rand_distr::{Distribution, LogNormal, Normal, Triangular};
use simorph_core::{Result, SimorphError, Uncertainty};

/// Draw one sample of a quantity with nominal `value` and uncertainty `u`.
///
/// For `Normal`, `Uniform`, `LogNormal`, `Triangular`, `Samples`, `Interval`
/// the sample follows the declared distribution.  `Exact` returns the
/// nominal value.  `Undeclared` is an **error**: SIMORPH never silently
/// treats an undeclared uncertainty as zero.
pub fn sample_uncertainty(value: f64, u: &Uncertainty, rng: &mut Rng) -> Result<f64> {
    Ok(match u {
        Uncertainty::Undeclared => {
            return Err(SimorphError::InvalidInput(
                "cannot sample a quantity with undeclared uncertainty".into(),
            ))
        }
        Uncertainty::Exact => value,
        Uncertainty::Normal { std_dev } => {
            if *std_dev == 0.0 {
                value
            } else {
                Normal::new(value, *std_dev)
                    .map_err(|e| SimorphError::InvalidInput(e.to_string()))?
                    .sample(rng)
            }
        }
        Uncertainty::Uniform { low, high } | Uncertainty::Interval { low, high } => {
            if high < low {
                return Err(SimorphError::InvalidInput("interval high < low".into()));
            }
            if high == low {
                *low
            } else {
                rng.gen_range(*low..*high)
            }
        }
        Uncertainty::LogNormal { mu, sigma } => LogNormal::new(*mu, *sigma)
            .map_err(|e| SimorphError::InvalidInput(e.to_string()))?
            .sample(rng),
        Uncertainty::Triangular { low, mode, high } => Triangular::new(*low, *high, *mode)
            .map_err(|e| SimorphError::InvalidInput(e.to_string()))?
            .sample(rng),
        Uncertainty::Samples { values } => {
            if values.is_empty() {
                return Err(SimorphError::InvalidInput("empty sample set".into()));
            }
            *values.choose(rng).unwrap()
        }
    })
}

/// Inverse CDF (quantile) of the uncertainty model, used by stratified
/// designs (LHS, quasi-random).  `p` in (0, 1).
pub fn inverse_cdf(value: f64, u: &Uncertainty, p: f64) -> Result<f64> {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    Ok(match u {
        Uncertainty::Undeclared => {
            return Err(SimorphError::InvalidInput(
                "cannot sample a quantity with undeclared uncertainty".into(),
            ))
        }
        Uncertainty::Exact => value,
        Uncertainty::Normal { std_dev } => value + std_dev * normal_quantile(p),
        Uncertainty::Uniform { low, high } | Uncertainty::Interval { low, high } => {
            low + (high - low) * p
        }
        Uncertainty::LogNormal { mu, sigma } => (mu + sigma * normal_quantile(p)).exp(),
        Uncertainty::Triangular { low, mode, high } => {
            let fc = (mode - low) / (high - low);
            if p < fc {
                low + ((high - low) * (mode - low) * p).sqrt()
            } else {
                high - ((high - low) * (high - mode) * (1.0 - p)).sqrt()
            }
        }
        Uncertainty::Samples { values } => {
            if values.is_empty() {
                return Err(SimorphError::InvalidInput("empty sample set".into()));
            }
            let mut s = values.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            crate::stats::quantile(&s, p)
        }
    })
}

/// Standard normal quantile (Acklam's rational approximation, |err| < 1.2e-9).
pub fn normal_quantile(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let plow = 0.02425;
    let phigh = 1.0 - plow;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= phigh {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// An input variable of a sampling design.
#[derive(Debug, Clone)]
pub struct InputSpec {
    pub name: String,
    pub value: f64,
    pub uncertainty: Uncertainty,
}

impl InputSpec {
    pub fn new(name: &str, value: f64, uncertainty: Uncertainty) -> Self {
        Self {
            name: name.into(),
            value,
            uncertainty,
        }
    }
    pub fn uniform(name: &str, low: f64, high: f64) -> Self {
        Self::new(name, 0.5 * (low + high), Uncertainty::Uniform { low, high })
    }
    pub fn normal(name: &str, mean: f64, std_dev: f64) -> Self {
        Self::new(name, mean, Uncertainty::Normal { std_dev })
    }
}

/// Sampling design kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Design {
    /// Independent pseudo-random draws.
    MonteCarlo,
    /// Latin hypercube with random permutation per dimension.
    LatinHypercube,
    /// Sobol sequence (Joe–Kuo direction numbers, up to 16 dimensions; Halton
    /// beyond that).
    Sobol,
    /// Halton sequence (prime bases).
    Halton,
}

/// Unit-hypercube points `[0,1)^d` for a design.
pub fn unit_design(design: Design, n: usize, d: usize, seed: u64) -> Vec<Vec<f64>> {
    match design {
        Design::MonteCarlo => {
            let mut rng = seeded(seed);
            (0..n)
                .map(|_| (0..d).map(|_| rng.gen::<f64>()).collect())
                .collect()
        }
        Design::LatinHypercube => latin_hypercube(n, d, seed),
        Design::Sobol => {
            if d <= SOBOL_MAX_DIM {
                sobol(n, d, true)
            } else {
                halton(n, d, true)
            }
        }
        Design::Halton => halton(n, d, true),
    }
}

/// Map unit-hypercube points to the input distributions via inverse CDFs.
pub fn map_to_inputs(points: &[Vec<f64>], inputs: &[InputSpec]) -> Result<Vec<Vec<f64>>> {
    points
        .iter()
        .map(|p| {
            p.iter()
                .zip(inputs)
                .map(|(&u, spec)| inverse_cdf(spec.value, &spec.uncertainty, u))
                .collect()
        })
        .collect()
}

/// Generate `n` input samples with the given design.
pub fn sample_inputs(
    design: Design,
    inputs: &[InputSpec],
    n: usize,
    seed: u64,
) -> Result<Vec<Vec<f64>>> {
    if design == Design::MonteCarlo {
        let mut rng = seeded(seed);
        return (0..n)
            .map(|_| {
                inputs
                    .iter()
                    .map(|s| sample_uncertainty(s.value, &s.uncertainty, &mut rng))
                    .collect()
            })
            .collect();
    }
    map_to_inputs(&unit_design(design, n, inputs.len(), seed), inputs)
}

/// Latin hypercube: each dimension is divided into `n` equal strata and
/// each stratum contains exactly one point.
pub fn latin_hypercube(n: usize, d: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut rng = seeded(seed);
    let mut cols: Vec<Vec<f64>> = Vec::with_capacity(d);
    for _ in 0..d {
        let mut perm: Vec<usize> = (0..n).collect();
        perm.shuffle(&mut rng);
        cols.push(
            perm.iter()
                .map(|&k| (k as f64 + rng.gen::<f64>()) / n as f64)
                .collect(),
        );
    }
    (0..n)
        .map(|i| (0..d).map(|j| cols[j][i]).collect())
        .collect()
}

/// Induce a target rank correlation matrix on an independent design using
/// the Iman–Conover method.  `target` is a d×d correlation matrix.
pub fn iman_conover(points: &[Vec<f64>], target: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
    use nalgebra::DMatrix;
    let n = points.len();
    let d = points.first().map(|p| p.len()).unwrap_or(0);
    if n < 2 || d == 0 || target.len() != d {
        return Err(SimorphError::InvalidInput(
            "iman_conover: bad dimensions".into(),
        ));
    }
    let c = DMatrix::from_fn(d, d, |i, j| target[i][j]);
    let chol = nalgebra::Cholesky::new(c).ok_or_else(|| {
        SimorphError::InvalidInput("target correlation not positive definite".into())
    })?;
    let p = chol.l();
    // van der Waerden scores
    let mut scores: Vec<f64> = (1..=n)
        .map(|i| normal_quantile(i as f64 / (n as f64 + 1.0)))
        .collect();
    let mut rng = seeded(7);
    let mut m = DMatrix::zeros(n, d);
    for j in 0..d {
        scores.shuffle(&mut rng);
        for i in 0..n {
            m[(i, j)] = scores[i];
        }
    }
    // Decorrelate M then impose P.
    let cm = (m.transpose() * &m) / n as f64;
    let chol_m = nalgebra::Cholesky::new(cm)
        .ok_or_else(|| SimorphError::Numerical("score matrix singular".into()))?;
    let q_inv = chol_m
        .l()
        .try_inverse()
        .ok_or_else(|| SimorphError::Numerical("score matrix singular".into()))?;
    let t = m * q_inv.transpose() * p.transpose();
    // Rank-reorder each column of `points` according to ranks of T.
    let mut out = vec![vec![0.0; d]; n];
    for j in 0..d {
        let mut col: Vec<f64> = points.iter().map(|p| p[j]).collect();
        col.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| t[(a, j)].partial_cmp(&t[(b, j)]).unwrap());
        for (rank, &row) in idx.iter().enumerate() {
            out[row][j] = col[rank];
        }
    }
    Ok(out)
}

/// First `n` primes.
fn primes(n: usize) -> Vec<u64> {
    let mut out = vec![];
    let mut c = 2u64;
    while out.len() < n {
        if out.iter().all(|p| c % p != 0) {
            out.push(c);
        }
        c += 1;
    }
    out
}

fn radical_inverse(mut i: u64, base: u64) -> f64 {
    let mut f = 1.0 / base as f64;
    let mut r = 0.0;
    while i > 0 {
        r += f * (i % base) as f64;
        i /= base;
        f /= base as f64;
    }
    r
}

/// Halton sequence; `skip_zero` drops the origin point.
pub fn halton(n: usize, d: usize, skip_zero: bool) -> Vec<Vec<f64>> {
    let bases = primes(d);
    let off = if skip_zero { 1 } else { 0 };
    (0..n)
        .map(|i| {
            bases
                .iter()
                .map(|&b| radical_inverse((i + off) as u64, b))
                .collect()
        })
        .collect()
}

pub const SOBOL_MAX_DIM: usize = 16;

/// Joe–Kuo direction number parameters (s, a, m) for dimensions 2..=16.
const SOBOL_PARAMS: [(u32, u32, &[u32]); 15] = [
    (1, 0, &[1]),
    (2, 1, &[1, 3]),
    (3, 1, &[1, 3, 1]),
    (3, 2, &[1, 1, 1]),
    (4, 1, &[1, 1, 3, 3]),
    (4, 4, &[1, 3, 5, 13]),
    (5, 2, &[1, 1, 5, 5, 17]),
    (5, 4, &[1, 1, 5, 5, 5]),
    (5, 7, &[1, 1, 7, 11, 19]),
    (5, 11, &[1, 1, 5, 1, 1]),
    (5, 13, &[1, 1, 1, 3, 11]),
    (5, 14, &[1, 3, 5, 5, 31]),
    (6, 1, &[1, 3, 3, 9, 7, 49]),
    (6, 13, &[1, 1, 1, 15, 21, 21]),
    (6, 16, &[1, 3, 1, 13, 27, 49]),
];

/// Sobol sequence (Gray-code construction) for up to [`SOBOL_MAX_DIM`]
/// dimensions.  Panics for larger `d` (callers use `unit_design`).
pub fn sobol(n: usize, d: usize, skip_zero: bool) -> Vec<Vec<f64>> {
    assert!(
        d <= SOBOL_MAX_DIM,
        "sobol: at most {SOBOL_MAX_DIM} dimensions"
    );
    const BITS: usize = 32;
    let total = n + if skip_zero { 1 } else { 0 };
    let l = (usize::BITS - total.max(1).leading_zeros()) as usize + 1;
    let l = l.min(BITS);
    // direction numbers v[j][k], scaled by 2^32
    let mut v = vec![vec![0u64; l + 1]; d];
    for k in 1..=l {
        v[0][k] = 1u64 << (BITS - k);
    }
    for j in 1..d {
        let (s, a, m) = SOBOL_PARAMS[j - 1];
        let s = s as usize;
        for k in 1..=l {
            if k <= s {
                v[j][k] = (m[k - 1] as u64) << (BITS - k);
            } else {
                let mut val = v[j][k - s] ^ (v[j][k - s] >> s);
                for i in 1..s {
                    if (a >> (s - 1 - i)) & 1 == 1 {
                        val ^= v[j][k - i];
                    }
                }
                v[j][k] = val;
            }
        }
    }
    let mut x = vec![0u64; d];
    let mut out = Vec::with_capacity(n);
    if !skip_zero {
        out.push(vec![0.0; d]);
    }
    let scale = 1.0 / (1u64 << BITS) as f64;
    let mut i: u64 = 0;
    while out.len() < n {
        // index of rightmost zero bit of i
        let c = (!i).trailing_zeros() as usize + 1;
        for j in 0..d {
            x[j] ^= v[j][c.min(l)];
        }
        i += 1;
        out.push(x.iter().map(|&xv| xv as f64 * scale).collect());
    }
    out
}

/// Full factorial grid with `levels` per dimension over [0,1].
pub fn full_factorial(levels: &[usize]) -> Vec<Vec<f64>> {
    let total: usize = levels.iter().product();
    (0..total)
        .map(|mut idx| {
            levels
                .iter()
                .map(|&lv| {
                    let k = idx % lv;
                    idx /= lv;
                    if lv == 1 {
                        0.5
                    } else {
                        k as f64 / (lv - 1) as f64
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{mean, variance};

    #[test]
    fn undeclared_uncertainty_is_an_error() {
        let mut r = seeded(1);
        assert!(sample_uncertainty(1.0, &Uncertainty::Undeclared, &mut r).is_err());
        assert_eq!(
            sample_uncertainty(1.0, &Uncertainty::Exact, &mut r).unwrap(),
            1.0
        );
    }

    #[test]
    fn normal_sampling_matches_moments() {
        let mut r = seeded(3);
        let x: Vec<f64> = (0..20000)
            .map(|_| {
                sample_uncertainty(5.0, &Uncertainty::Normal { std_dev: 2.0 }, &mut r).unwrap()
            })
            .collect();
        assert!((mean(&x) - 5.0).abs() < 0.05);
        assert!((variance(&x).sqrt() - 2.0).abs() < 0.05);
    }

    #[test]
    fn normal_quantile_accuracy() {
        assert!((normal_quantile(0.5)).abs() < 1e-9);
        assert!((normal_quantile(0.975) - 1.959963984540054).abs() < 1e-8);
        assert!((normal_quantile(0.001) + 3.090232306167813).abs() < 1e-8);
    }

    #[test]
    fn triangular_inverse_cdf_median() {
        let u = Uncertainty::Triangular {
            low: 0.0,
            mode: 1.0,
            high: 2.0,
        };
        assert!((inverse_cdf(0.0, &u, 0.5).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn lhs_is_stratified() {
        let n = 50;
        let pts = latin_hypercube(n, 3, 9);
        for j in 0..3 {
            let mut strata = vec![0; n];
            for p in &pts {
                strata[(p[j] * n as f64).floor() as usize] += 1;
            }
            assert!(
                strata.iter().all(|&c| c == 1),
                "dimension {j} not stratified"
            );
        }
    }

    #[test]
    fn sobol_first_two_dims_form_a_net() {
        // The first 256 points of a 2-D Sobol sequence (including the origin)
        // place exactly one point in each cell of a 16x16 grid.
        let pts = sobol(256, 2, false);
        let mut cells = vec![0; 256];
        for p in &pts {
            let i = (p[0] * 16.0).floor() as usize;
            let j = (p[1] * 16.0).floor() as usize;
            cells[i * 16 + j] += 1;
        }
        assert!(cells.iter().all(|&c| c == 1));
        // every dimension fills the 256 strata of [0,1) exactly once
        let pts = sobol(256, 16, false);
        for j in 0..16 {
            let mut strata = vec![0; 256];
            for p in &pts {
                strata[(p[j] * 256.0).floor() as usize] += 1;
            }
            assert!(
                strata.iter().all(|&c| c == 1),
                "sobol dim {j} not a (0,1)-sequence"
            );
        }
    }

    #[test]
    fn halton_is_low_discrepancy() {
        let n = 1000;
        let pts = halton(n, 2, true);
        // Count points in [0,0.5)^2: should be ~n/4 with error much smaller than MC.
        let c = pts.iter().filter(|p| p[0] < 0.5 && p[1] < 0.5).count();
        assert!((c as f64 - n as f64 / 4.0).abs() < 5.0);
    }

    #[test]
    fn iman_conover_induces_correlation() {
        let pts = latin_hypercube(400, 2, 11);
        let target = vec![vec![1.0, 0.8], vec![0.8, 1.0]];
        let out = iman_conover(&pts, &target).unwrap();
        let x: Vec<f64> = out.iter().map(|p| p[0]).collect();
        let y: Vec<f64> = out.iter().map(|p| p[1]).collect();
        let r = crate::stats::correlation(&x, &y);
        assert!((r - 0.8).abs() < 0.05, "r={r}");
        // marginals are preserved (same multiset)
        let mut a: Vec<f64> = pts.iter().map(|p| p[0]).collect();
        let mut b = x.clone();
        a.sort_by(|p, q| p.partial_cmp(q).unwrap());
        b.sort_by(|p, q| p.partial_cmp(q).unwrap());
        assert_eq!(a, b);
    }

    #[test]
    fn full_factorial_size() {
        let g = full_factorial(&[3, 2]);
        assert_eq!(g.len(), 6);
        assert!(g.contains(&vec![0.5, 0.0]));
    }

    #[test]
    fn designs_are_deterministic() {
        let a = unit_design(Design::LatinHypercube, 10, 3, 5);
        let b = unit_design(Design::LatinHypercube, 10, 3, 5);
        assert_eq!(a, b);
    }
}
