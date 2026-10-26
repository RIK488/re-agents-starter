//! Designs of experiments.

use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};

/// A factor (input) explored by a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Factor {
    pub name: String,
    pub unit: String,
    pub low: f64,
    pub high: f64,
    /// Number of levels for factorial designs (>= 2).
    #[serde(default = "two")]
    pub levels: usize,
    /// Nominal value for one-at-a-time designs (defaults to the midpoint).
    #[serde(default)]
    pub nominal: Option<f64>,
}

fn two() -> usize {
    2
}

impl Factor {
    pub fn new(name: &str, unit: &str, low: f64, high: f64) -> Self {
        Self {
            name: name.into(),
            unit: unit.into(),
            low,
            high,
            levels: 2,
            nominal: None,
        }
    }
    pub fn with_levels(mut self, n: usize) -> Self {
        self.levels = n;
        self
    }
    pub fn nominal_value(&self) -> f64 {
        self.nominal.unwrap_or(0.5 * (self.low + self.high))
    }
    fn level_value(&self, i: usize) -> f64 {
        if self.levels <= 1 {
            return self.nominal_value();
        }
        self.low + (self.high - self.low) * i as f64 / (self.levels - 1) as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DesignKind {
    FullFactorial,
    LatinHypercube,
    UniformRandom,
    OneAtATime,
    CentralComposite,
    PlackettBurman,
    /// Points supplied explicitly.
    Explicit,
}

/// A set of points in factor space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Design {
    pub kind: DesignKind,
    pub factors: Vec<Factor>,
    /// Each point has one value per factor, in factor order.
    pub points: Vec<Vec<f64>>,
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Design {
    pub fn len(&self) -> usize {
        self.points.len()
    }
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn check(factors: &[Factor]) -> Result<()> {
        if factors.is_empty() {
            return Err(SimorphError::InvalidInput(
                "design needs at least one factor".into(),
            ));
        }
        for f in factors {
            if !(f.low <= f.high) {
                return Err(SimorphError::InvalidInput(format!(
                    "factor `{}` has low > high",
                    f.name
                )));
            }
        }
        Ok(())
    }

    /// Every combination of levels.  Size = Π levels.
    pub fn full_factorial(factors: Vec<Factor>) -> Result<Self> {
        Self::check(&factors)?;
        let mut points: Vec<Vec<f64>> = vec![vec![]];
        for f in &factors {
            let n = f.levels.max(1);
            let mut next = Vec::with_capacity(points.len() * n);
            for p in &points {
                for i in 0..n {
                    let mut q = p.clone();
                    q.push(f.level_value(i));
                    next.push(q);
                }
            }
            points = next;
        }
        Ok(Self {
            kind: DesignKind::FullFactorial,
            factors,
            points,
            seed: None,
        })
    }

    /// Latin hypercube: `n` points, each factor stratified into `n` equal
    /// intervals with exactly one point per interval.
    pub fn latin_hypercube(factors: Vec<Factor>, n: usize, seed: u64) -> Result<Self> {
        Self::check(&factors)?;
        if n == 0 {
            return Err(SimorphError::InvalidInput("n must be > 0".into()));
        }
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let k = factors.len();
        let mut cols: Vec<Vec<f64>> = Vec::with_capacity(k);
        for f in &factors {
            let mut perm: Vec<usize> = (0..n).collect();
            perm.shuffle(&mut rng);
            let col: Vec<f64> = perm
                .iter()
                .map(|&cell| {
                    let u = (cell as f64 + rng.gen::<f64>()) / n as f64;
                    f.low + (f.high - f.low) * u
                })
                .collect();
            cols.push(col);
        }
        let points = (0..n)
            .map(|i| cols.iter().map(|c| c[i]).collect())
            .collect();
        Ok(Self {
            kind: DesignKind::LatinHypercube,
            factors,
            points,
            seed: Some(seed),
        })
    }

    /// `n` independent uniform points.
    pub fn uniform_random(factors: Vec<Factor>, n: usize, seed: u64) -> Result<Self> {
        Self::check(&factors)?;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let points = (0..n)
            .map(|_| {
                factors
                    .iter()
                    .map(|f| f.low + (f.high - f.low) * rng.gen::<f64>())
                    .collect()
            })
            .collect();
        Ok(Self {
            kind: DesignKind::UniformRandom,
            factors,
            points,
            seed: Some(seed),
        })
    }

    /// Nominal point plus, for each factor, its levels with the others held
    /// at nominal.  Size = 1 + Σ levels.
    pub fn one_at_a_time(factors: Vec<Factor>) -> Result<Self> {
        Self::check(&factors)?;
        let nominal: Vec<f64> = factors.iter().map(|f| f.nominal_value()).collect();
        let mut points = vec![nominal.clone()];
        for (j, f) in factors.iter().enumerate() {
            for i in 0..f.levels.max(2) {
                let mut p = nominal.clone();
                p[j] = f.level_value(i);
                points.push(p);
            }
        }
        Ok(Self {
            kind: DesignKind::OneAtATime,
            factors,
            points,
            seed: None,
        })
    }

    /// Face-centred central composite design for 2–4 factors:
    /// 2^k corner points, 2k axial (face) points, one centre point.
    pub fn central_composite(factors: Vec<Factor>) -> Result<Self> {
        Self::check(&factors)?;
        let k = factors.len();
        if !(2..=4).contains(&k) {
            return Err(SimorphError::InvalidInput(
                "central composite design supports 2 to 4 factors".into(),
            ));
        }
        let centre: Vec<f64> = factors.iter().map(|f| 0.5 * (f.low + f.high)).collect();
        let mut points = vec![];
        for mask in 0..(1usize << k) {
            points.push(
                factors
                    .iter()
                    .enumerate()
                    .map(|(j, f)| if mask & (1 << j) == 0 { f.low } else { f.high })
                    .collect(),
            );
        }
        for j in 0..k {
            let mut lo = centre.clone();
            lo[j] = factors[j].low;
            let mut hi = centre.clone();
            hi[j] = factors[j].high;
            points.push(lo);
            points.push(hi);
        }
        points.push(centre);
        Ok(Self {
            kind: DesignKind::CentralComposite,
            factors,
            points,
            seed: None,
        })
    }

    /// Plackett–Burman two-level screening design for up to 11 factors
    /// (12-run design, generated from the standard first row by cyclic
    /// shifts plus a row of lows).
    pub fn plackett_burman(factors: Vec<Factor>) -> Result<Self> {
        Self::check(&factors)?;
        let k = factors.len();
        if k > 11 {
            return Err(SimorphError::InvalidInput(
                "Plackett–Burman implementation supports up to 11 factors".into(),
            ));
        }
        // Standard 12-run generator row (+ = high, - = low).
        let gen: [i8; 11] = [1, 1, -1, 1, 1, 1, -1, -1, -1, 1, -1];
        let mut rows: Vec<Vec<i8>> = vec![];
        for shift in 0..11 {
            let row: Vec<i8> = (0..11).map(|j| gen[(j + 11 - shift) % 11]).collect();
            rows.push(row);
        }
        rows.push(vec![-1; 11]);
        let points = rows
            .iter()
            .map(|r| {
                factors
                    .iter()
                    .enumerate()
                    .map(|(j, f)| if r[j] > 0 { f.high } else { f.low })
                    .collect()
            })
            .collect();
        Ok(Self {
            kind: DesignKind::PlackettBurman,
            factors,
            points,
            seed: None,
        })
    }

    pub fn explicit(factors: Vec<Factor>, points: Vec<Vec<f64>>) -> Result<Self> {
        Self::check(&factors)?;
        for p in &points {
            if p.len() != factors.len() {
                return Err(SimorphError::InvalidInput(
                    "explicit point has wrong arity".into(),
                ));
            }
        }
        Ok(Self {
            kind: DesignKind::Explicit,
            factors,
            points,
            seed: None,
        })
    }

    /// Column `j` of the design.
    pub fn column(&self, j: usize) -> Vec<f64> {
        self.points.iter().map(|p| p[j]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs() -> Vec<Factor> {
        vec![
            Factor::new("a", "1", 0.0, 1.0).with_levels(3),
            Factor::new("b", "m", -1.0, 1.0),
        ]
    }

    #[test]
    fn sizes() {
        assert_eq!(Design::full_factorial(fs()).unwrap().len(), 6);
        assert_eq!(Design::one_at_a_time(fs()).unwrap().len(), 1 + 3 + 2);
        assert_eq!(Design::central_composite(fs()).unwrap().len(), 4 + 4 + 1);
        assert_eq!(Design::plackett_burman(fs()).unwrap().len(), 12);
        assert_eq!(Design::uniform_random(fs(), 17, 1).unwrap().len(), 17);
        assert!(Design::central_composite(vec![Factor::new("a", "1", 0.0, 1.0)]).is_err());
    }

    #[test]
    fn latin_hypercube_is_stratified_and_seeded() {
        let n = 40;
        let d = Design::latin_hypercube(fs(), n, 7).unwrap();
        for j in 0..2 {
            let f = &d.factors[j];
            let mut cells: Vec<usize> = d
                .column(j)
                .iter()
                .map(|v| (((v - f.low) / (f.high - f.low)) * n as f64).floor() as usize)
                .collect();
            cells.sort_unstable();
            assert_eq!(
                cells,
                (0..n).collect::<Vec<_>>(),
                "factor {j} not stratified"
            );
        }
        let d2 = Design::latin_hypercube(fs(), n, 7).unwrap();
        assert_eq!(d, d2);
        let d3 = Design::latin_hypercube(fs(), n, 8).unwrap();
        assert_ne!(d, d3);
    }

    #[test]
    fn plackett_burman_columns_are_balanced_and_orthogonal() {
        let factors: Vec<Factor> = (0..11)
            .map(|i| Factor::new(&format!("f{i}"), "1", -1.0, 1.0))
            .collect();
        let d = Design::plackett_burman(factors).unwrap();
        for j in 0..11 {
            assert_eq!(d.column(j).iter().sum::<f64>(), 0.0);
            for k in (j + 1)..11 {
                let dot: f64 = d
                    .column(j)
                    .iter()
                    .zip(d.column(k))
                    .map(|(a, b)| a * b)
                    .sum();
                assert_eq!(dot, 0.0, "columns {j},{k} not orthogonal");
            }
        }
    }
}
