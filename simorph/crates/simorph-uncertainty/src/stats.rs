//! Descriptive statistics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Summary statistics of a sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub n: usize,
    pub mean: f64,
    pub variance: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    /// (p, quantile) pairs for p in 0.025, 0.05, 0.25, 0.5, 0.75, 0.95, 0.975.
    pub quantiles: Vec<(f64, f64)>,
    /// 95% confidence interval for the mean (normal approximation).
    pub mean_ci95: (f64, f64),
    /// Histogram as (bin_left_edge, count).
    pub histogram: Vec<(f64, usize)>,
    pub bin_width: f64,
}

pub fn mean(x: &[f64]) -> f64 {
    if x.is_empty() {
        return f64::NAN;
    }
    x.iter().sum::<f64>() / x.len() as f64
}

/// Unbiased sample variance.
pub fn variance(x: &[f64]) -> f64 {
    let n = x.len();
    if n < 2 {
        return 0.0;
    }
    let m = mean(x);
    x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (n as f64 - 1.0)
}

/// Quantile by linear interpolation on sorted data.
pub fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let w = pos - lo as f64;
    sorted[lo] * (1.0 - w) + sorted[hi] * w
}

pub fn summarize(x: &[f64]) -> Summary {
    summarize_bins(x, 20)
}

pub fn summarize_bins(x: &[f64], bins: usize) -> Summary {
    let n = x.len();
    let mut sorted = x.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = mean(x);
    let var = variance(x);
    let sd = var.sqrt();
    let min = sorted.first().copied().unwrap_or(f64::NAN);
    let max = sorted.last().copied().unwrap_or(f64::NAN);
    let se = if n > 0 {
        sd / (n as f64).sqrt()
    } else {
        f64::NAN
    };
    let quantiles = [0.025, 0.05, 0.25, 0.5, 0.75, 0.95, 0.975]
        .iter()
        .map(|&p| (p, quantile(&sorted, p)))
        .collect();
    let bins = bins.max(1);
    let bin_width = if max > min {
        (max - min) / bins as f64
    } else {
        1.0
    };
    let mut histogram: Vec<(f64, usize)> = (0..bins)
        .map(|i| (min + i as f64 * bin_width, 0usize))
        .collect();
    for &v in x {
        let mut i = ((v - min) / bin_width).floor() as usize;
        if i >= bins {
            i = bins - 1;
        }
        histogram[i].1 += 1;
    }
    Summary {
        n,
        mean: m,
        variance: var,
        std_dev: sd,
        min,
        max,
        quantiles,
        mean_ci95: (m - 1.96 * se, m + 1.96 * se),
        histogram,
        bin_width,
    }
}

/// Pearson correlation.
pub fn correlation(x: &[f64], y: &[f64]) -> f64 {
    let mx = mean(x);
    let my = mean(y);
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx).powi(2);
        syy += (b - my).powi(2);
    }
    sxy / (sxx * syy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_summary() {
        let s = summarize(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(s.mean, 3.0);
        assert_eq!(s.variance, 2.5);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 5.0);
        let med = s.quantiles.iter().find(|q| q.0 == 0.5).unwrap().1;
        assert_eq!(med, 3.0);
        assert_eq!(s.histogram.iter().map(|h| h.1).sum::<usize>(), 5);
    }
}
