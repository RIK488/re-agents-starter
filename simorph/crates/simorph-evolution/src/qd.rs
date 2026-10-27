//! MAP-Elites quality-diversity search over expression programs.
//!
//! The archive keeps, for every cell of a discretised behaviour space, the
//! best individual whose behaviour falls in that cell.  Parents are drawn
//! from occupied cells with probability proportional to
//! `1 + novelty_weight · novelty`, where novelty is the mean Euclidean
//! distance (in cell coordinates) to the `k` nearest other occupied cells,
//! so sparsely populated regions of behaviour space are explored more.

use crate::expr::Expr;
use crate::gp::{optimise_constants, Ctx, Dataset, GpConfig, Individual};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result};
use std::collections::BTreeMap;

/// Behaviour descriptor: maps an individual to integer cell coordinates.
pub type Descriptor = dyn Fn(&Individual) -> Vec<usize> + Sync;

/// Default descriptor: (size bucket of width `size_bin`, variable-use mask).
pub fn size_and_variables(size_bin: usize) -> impl Fn(&Individual) -> Vec<usize> + Sync {
    move |ind| vec![ind.size / size_bin.max(1), ind.expr.var_mask() as usize]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapElitesConfig {
    pub gp: GpConfig,
    pub initial: usize,
    pub iterations: usize,
    pub batch: usize,
    pub novelty_weight: f64,
    pub k_nearest: usize,
}

impl MapElitesConfig {
    pub fn new(gp: GpConfig) -> Self {
        Self {
            gp,
            initial: 400,
            iterations: 60,
            batch: 100,
            novelty_weight: 1.0,
            k_nearest: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapElitesResult {
    pub cells: BTreeMap<Vec<usize>, Individual>,
    pub coverage: usize,
    /// Σ over cells of max(0, 1 − error): rewards both quality and diversity.
    pub qd_score: f64,
    pub best: Individual,
    pub evaluations: usize,
    /// Coverage after each iteration.
    pub coverage_history: Vec<usize>,
}

fn novelty(cells: &BTreeMap<Vec<usize>, Individual>, key: &[usize], k: usize) -> f64 {
    let mut d: Vec<f64> = cells
        .keys()
        .filter(|c| c.as_slice() != key)
        .map(|c| {
            c.iter()
                .zip(key)
                .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    d.sort_by(|a, b| a.total_cmp(b));
    let k = k.min(d.len());
    if k == 0 {
        0.0
    } else {
        d[..k].iter().sum::<f64>() / k as f64
    }
}

fn insert(cells: &mut BTreeMap<Vec<usize>, Individual>, key: Vec<usize>, ind: Individual) -> bool {
    if !ind.fitness.is_finite() {
        return false;
    }
    match cells.get(&key) {
        Some(old) if old.fitness <= ind.fitness => false,
        _ => {
            cells.insert(key, ind);
            true
        }
    }
}

pub fn map_elites(
    data: &Dataset,
    cfg: &MapElitesConfig,
    descriptor: &Descriptor,
    ctl: &ExecutionControl,
) -> Result<MapElitesResult> {
    let ctx = Ctx::new(data, &cfg.gp)?;
    let ps = &cfg.gp.primitives;
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.gp.seed);
    let mut cells: BTreeMap<Vec<usize>, Individual> = BTreeMap::new();
    let mut evaluations = 0;
    for i in 0..cfg.initial {
        let e = Expr::ramped(ps, cfg.gp.init_depth.0, cfg.gp.init_depth.1, i, &mut rng);
        if !ctx.admissible(&e) {
            continue;
        }
        let ind = ctx.evaluate(e);
        evaluations += 1;
        let key = descriptor(&ind);
        insert(&mut cells, key, ind);
    }
    let mut coverage_history = vec![cells.len()];
    for _ in 0..cfg.iterations {
        if ctl.check().is_err() || cells.is_empty() {
            break;
        }
        let keys: Vec<Vec<usize>> = cells.keys().cloned().collect();
        let weights: Vec<f64> = keys
            .iter()
            .map(|k| 1.0 + cfg.novelty_weight * novelty(&cells, k, cfg.k_nearest))
            .collect();
        let total: f64 = weights.iter().sum();
        let pick = |rng: &mut ChaCha8Rng| {
            let mut r = rng.gen::<f64>() * total;
            for (k, w) in keys.iter().zip(&weights) {
                if r < *w {
                    return k.clone();
                }
                r -= w;
            }
            keys[keys.len() - 1].clone()
        };
        for _ in 0..cfg.batch {
            let p1 = cells[&pick(&mut rng)].expr.clone();
            let r: f64 = rng.gen();
            let child = if r < 0.5 {
                let p2 = cells[&pick(&mut rng)].expr.clone();
                p1.crossover(&p2, &mut rng)
            } else if r < 0.75 {
                p1.subtree_mutation(ps, 3, &mut rng)
            } else if r < 0.9 {
                p1.point_mutation(ps, &mut rng)
            } else {
                p1.constant_mutation(0.2, &mut rng)
            };
            if !ctx.admissible(&child) {
                continue;
            }
            let mut ind = ctx.evaluate(child);
            evaluations += 1;
            if rng.gen::<f64>() < 0.05 {
                let (o, ev) = optimise_constants(&ctx, &ind, cfg.gp.const_opt_evals);
                ind = o;
                evaluations += ev;
            }
            let key = descriptor(&ind);
            insert(&mut cells, key, ind);
        }
        coverage_history.push(cells.len());
    }
    let best = cells
        .values()
        .min_by(|a, b| a.fitness.total_cmp(&b.fitness))
        .cloned()
        .ok_or_else(|| {
            simorph_core::SimorphError::Numerical("MAP-Elites archive is empty".into())
        })?;
    let qd_score = cells.values().map(|i| (1.0 - i.error).max(0.0)).sum();
    Ok(MapElitesResult {
        coverage: cells.len(),
        qd_score,
        best,
        evaluations,
        coverage_history,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::PrimitiveSet;

    fn data() -> Dataset {
        let mut d = Dataset::new(&["x0", "x1", "x2"], &["1", "1", "1"], "y", "1");
        for i in 0..6 {
            for j in 0..5 {
                for k in 0..3 {
                    let (a, b, c) = (0.5 + i as f64 * 0.5, 1.0 + j as f64 * 0.4, k as f64);
                    d.push(vec![a, b, c], a * a + b);
                }
            }
        }
        d
    }

    #[test]
    fn archive_is_diverse_elitist_and_finds_the_target() {
        let d = data();
        let mut gp = GpConfig::new(PrimitiveSet::arithmetic(3), 5);
        gp.max_size = 20;
        let mut cfg = MapElitesConfig::new(gp);
        cfg.iterations = std::env::var("ME_IT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);
        let desc = size_and_variables(3);
        let r = map_elites(&d, &cfg, &desc, &ExecutionControl::default()).unwrap();
        // invariant: every elite sits in its own descriptor cell
        for (k, ind) in &r.cells {
            assert_eq!(&desc(ind), k);
        }
        // coverage never decreases
        for w in r.coverage_history.windows(2) {
            assert!(w[1] >= w[0]);
        }
        // several variable subsets are represented
        let masks: std::collections::BTreeSet<usize> = r.cells.keys().map(|k| k[1]).collect();
        assert!(masks.len() >= 5, "{masks:?}");
        assert!(
            r.best.error < 1e-6,
            "best {} err {}",
            r.best.model,
            r.best.error
        );
        // the best elite does not use the distractor x2
        assert_eq!(r.best.expr.var_mask() & 0b100, 0);
        println!(
            "MAP-Elites: coverage {} cells, QD score {:.2}, {} evaluations, best {}",
            r.coverage,
            r.qd_score,
            r.evaluations,
            r.best.model.simplify()
        );
    }

    #[test]
    fn novelty_bonus_changes_exploration() {
        let d = data();
        let mut gp = GpConfig::new(PrimitiveSet::arithmetic(3), 9);
        gp.max_size = 20;
        let mut with = MapElitesConfig::new(gp.clone());
        with.iterations = 30;
        let mut without = with.clone();
        without.novelty_weight = 0.0;
        let desc = size_and_variables(2);
        let a = map_elites(&d, &with, &desc, &ExecutionControl::default()).unwrap();
        let b = map_elites(&d, &without, &desc, &ExecutionControl::default()).unwrap();
        println!(
            "coverage with novelty {} / without {}",
            a.coverage, b.coverage
        );
        assert_ne!(a.coverage_history, b.coverage_history);
    }
}
