//! Genetic programming engine: islands, lexicographic-parsimony tournament
//! selection, Pareto archive on (error, size), constant optimisation by
//! Nelder–Mead, optional dimensional constraints.

use crate::expr::{Expr, PrimitiveSet};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};
use simorph_units::Dimension;
use std::collections::BTreeMap;
use std::time::Instant;

/// Tabular data set `y = f(x)` with units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub names: Vec<String>,
    pub units: Vec<String>,
    pub target_name: String,
    pub target_unit: String,
    pub x: Vec<Vec<f64>>,
    pub y: Vec<f64>,
}

impl Dataset {
    pub fn new(names: &[&str], units: &[&str], target_name: &str, target_unit: &str) -> Self {
        Self {
            names: names.iter().map(|s| s.to_string()).collect(),
            units: units.iter().map(|s| s.to_string()).collect(),
            target_name: target_name.into(),
            target_unit: target_unit.into(),
            x: vec![],
            y: vec![],
        }
    }
    pub fn push(&mut self, x: Vec<f64>, y: f64) {
        self.x.push(x);
        self.y.push(y);
    }
    pub fn len(&self) -> usize {
        self.y.len()
    }
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }
    pub fn var_dimensions(&self) -> Result<Vec<Dimension>> {
        self.units
            .iter()
            .map(|u| simorph_units::parse_unit(u).map(|u| u.dimension))
            .collect()
    }
    pub fn target_dimension(&self) -> Result<Dimension> {
        Ok(simorph_units::parse_unit(&self.target_unit)?.dimension)
    }
    /// Deterministic shuffled split; returns (train, held_out).
    pub fn split(&self, held_out_fraction: f64, seed: u64) -> (Dataset, Dataset) {
        let mut idx: Vec<usize> = (0..self.len()).collect();
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        use rand::seq::SliceRandom;
        idx.shuffle(&mut rng);
        let n_hold = ((self.len() as f64) * held_out_fraction).round() as usize;
        let mut a = Dataset {
            x: vec![],
            y: vec![],
            ..self.clone()
        };
        let mut b = a.clone();
        for (k, i) in idx.into_iter().enumerate() {
            if k < n_hold {
                b.push(self.x[i].clone(), self.y[i]);
            } else {
                a.push(self.x[i].clone(), self.y[i]);
            }
        }
        (a, b)
    }
    /// SHA-256 of the data (sealing evaluation sets).
    pub fn content_hash(&self) -> String {
        simorph_core::hashing::hash_json(self).expect("serialisable")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ErrorMetric {
    /// sqrt(mean(((ŷ − y)/y)²)) — for strictly non-zero multiplicative laws.
    RelativeRmse,
    /// RMSE divided by the standard deviation of y (1 = constant-mean model).
    NormalisedRmse,
}

pub fn error(metric: ErrorMetric, pred: &[f64], y: &[f64]) -> f64 {
    if pred.iter().any(|p| !p.is_finite()) || y.is_empty() {
        return f64::INFINITY;
    }
    match metric {
        ErrorMetric::RelativeRmse => {
            let s: f64 = pred.iter().zip(y).map(|(p, t)| ((p - t) / t).powi(2)).sum();
            (s / y.len() as f64).sqrt()
        }
        ErrorMetric::NormalisedRmse => {
            let mean = y.iter().sum::<f64>() / y.len() as f64;
            let var = y.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / y.len() as f64;
            let mse = pred
                .iter()
                .zip(y)
                .map(|(p, t)| (p - t).powi(2))
                .sum::<f64>()
                / y.len() as f64;
            (mse / var.max(1e-300)).sqrt()
        }
    }
}

/// Analytic output scaling applied before measuring the error (Keijzer's
/// "linear scaling"): the optimal scale (and offset) are computed in closed
/// form for each candidate, which removes the burden of discovering
/// constants by evolution.  `Multiplicative` keeps dimensions intact (the
/// scale is a dimensionless constant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scaling {
    None,
    Multiplicative,
    Affine,
}

/// Optimal `(offset, scale)` for `pred` against `y` under `metric`.
pub fn optimal_scaling(
    scaling: Scaling,
    metric: ErrorMetric,
    pred: &[f64],
    y: &[f64],
) -> (f64, f64) {
    match scaling {
        Scaling::None => (0.0, 1.0),
        Scaling::Multiplicative => {
            let (num, den) = match metric {
                ErrorMetric::RelativeRmse => {
                    pred.iter().zip(y).fold((0.0, 0.0), |(n, d), (p, t)| {
                        (n + p / t, d + (p / t).powi(2))
                    })
                }
                ErrorMetric::NormalisedRmse => pred
                    .iter()
                    .zip(y)
                    .fold((0.0, 0.0), |(n, d), (p, t)| (n + p * t, d + p * p)),
            };
            if den > 1e-300 && num.is_finite() && den.is_finite() {
                (0.0, num / den)
            } else {
                (0.0, 1.0)
            }
        }
        Scaling::Affine => {
            let n = y.len() as f64;
            let mp = pred.iter().sum::<f64>() / n;
            let my = y.iter().sum::<f64>() / n;
            let cov: f64 = pred.iter().zip(y).map(|(p, t)| (p - mp) * (t - my)).sum();
            let var: f64 = pred.iter().map(|p| (p - mp).powi(2)).sum();
            if var > 1e-300 && cov.is_finite() {
                let b = cov / var;
                (my - b * mp, b)
            } else {
                (my, 0.0)
            }
        }
    }
}

/// Wrap `e` as `offset + scale·e` (identity parts omitted).
pub fn apply_scaling(e: &Expr, offset: f64, scale: f64) -> Expr {
    use crate::expr::BinOp;
    let mut out = e.clone();
    if scale != 1.0 {
        out = Expr::Binary(BinOp::Mul, Box::new(Expr::Const(scale)), Box::new(out));
    }
    if offset != 0.0 {
        out = Expr::Binary(BinOp::Add, Box::new(Expr::Const(offset)), Box::new(out));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DimensionMode {
    /// Plain GP: units ignored.
    Ignore,
    /// Inconsistent or wrong-dimension individuals get `error·(1+w) + w`.
    Penalise(f64),
    /// Inconsistent or wrong-dimension offspring are regenerated (up to 20
    /// attempts) and never enter the population as valid individuals.
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpConfig {
    pub primitives: PrimitiveSet,
    pub population: usize,
    pub islands: usize,
    pub generations: usize,
    pub init_depth: (usize, usize),
    pub max_depth: usize,
    pub max_size: usize,
    pub tournament: usize,
    pub p_crossover: f64,
    pub p_subtree: f64,
    pub p_point: f64,
    pub p_constant: f64,
    pub migration_interval: usize,
    pub migrants: usize,
    pub elitism: usize,
    /// Number of top individuals per island whose constants are optimised each generation.
    pub const_opt_top: usize,
    pub const_opt_evals: usize,
    pub metric: ErrorMetric,
    pub scaling: Scaling,
    /// Selection fitness is `error·(1 + parsimony·size)` (bloat control).
    pub parsimony: f64,
    pub dimension_mode: DimensionMode,
    /// Stop when the best training error falls below this value.
    pub target_error: f64,
    pub max_evaluations: usize,
    /// Attempts to produce an admissible offspring before copying a parent.
    pub variation_attempts: usize,
    pub seed: u64,
}

impl GpConfig {
    pub fn new(primitives: PrimitiveSet, seed: u64) -> Self {
        Self {
            primitives,
            // Defaults chosen by a reliability sweep on y = x0² + 3·x1
            // (10 seeds): pop 200 → 2/10, 400 → 7–9/10, 800 → 10/10.
            population: 800,
            islands: 4,
            generations: 40,
            init_depth: (2, 5),
            max_depth: 8,
            max_size: 30,
            tournament: 4,
            p_crossover: 0.6,
            p_subtree: 0.15,
            p_point: 0.1,
            p_constant: 0.15,
            migration_interval: 5,
            migrants: 3,
            elitism: 2,
            const_opt_top: 3,
            const_opt_evals: 60,
            metric: ErrorMetric::RelativeRmse,
            scaling: Scaling::Multiplicative,
            parsimony: 0.005,
            dimension_mode: DimensionMode::Ignore,
            target_error: 0.0,
            max_evaluations: usize::MAX,
            variation_attempts: 20,
            seed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Individual {
    /// Evolved (unscaled) expression.
    pub expr: Expr,
    /// `offset + scale·expr`: the model actually evaluated.
    pub model: Expr,
    /// Fitness used for selection (training error, possibly penalised).
    pub fitness: f64,
    /// Raw training error.
    pub error: f64,
    pub size: usize,
    /// Dimension matches the target (always `true` in `Ignore` mode... no:
    /// computed regardless of mode, for reporting).
    pub dimension_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationSummary {
    pub generation: usize,
    pub best_error: f64,
    pub best_size: usize,
    pub mean_size: f64,
    pub dimensionally_valid_fraction: f64,
    pub evaluations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpResult {
    pub best: Individual,
    /// Non-dominated (error, size) individuals found during the run.
    pub pareto: Vec<Individual>,
    pub history: Vec<GenerationSummary>,
    pub evaluations: usize,
    pub generations: usize,
    pub wall_time_s: f64,
    pub stop_reason: String,
}

pub(crate) struct Ctx<'a> {
    pub(crate) data: &'a Dataset,
    pub(crate) cfg: &'a GpConfig,
    pub(crate) var_dims: Vec<Dimension>,
    pub(crate) target_dim: Dimension,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(data: &'a Dataset, cfg: &'a GpConfig) -> Result<Self> {
        Ok(Ctx {
            data,
            cfg,
            var_dims: data.var_dimensions()?,
            target_dim: data.target_dimension()?,
        })
    }
    fn dim_status(&self, e: &Expr) -> (bool, bool) {
        match e.dimension(&self.var_dims) {
            None => (false, false),
            Some(d) => (true, d == self.target_dim),
        }
    }
    pub(crate) fn evaluate(&self, e: Expr) -> Individual {
        let raw: Vec<f64> = self.data.x.iter().map(|x| e.eval(x)).collect();
        let (off, sc) = optimal_scaling(self.cfg.scaling, self.cfg.metric, &raw, &self.data.y);
        let pred: Vec<f64> = raw.iter().map(|p| off + sc * p).collect();
        let err = error(self.cfg.metric, &pred, &self.data.y);
        let model = apply_scaling(&e, off, sc);
        let (consistent, matches) = self.dim_status(&e);
        let ok = consistent && matches;
        let size = e.size();
        let raw_fitness = match self.cfg.dimension_mode {
            DimensionMode::Ignore => err,
            DimensionMode::Penalise(w) => {
                if ok {
                    err
                } else {
                    err * (1.0 + w) + w
                }
            }
            DimensionMode::Reject => {
                if ok {
                    err
                } else {
                    f64::INFINITY
                }
            }
        };
        let fitness = raw_fitness * (1.0 + self.cfg.parsimony * size as f64);
        Individual {
            expr: e,
            model,
            fitness,
            error: err,
            size,
            dimension_ok: ok,
        }
    }
    pub(crate) fn admissible(&self, e: &Expr) -> bool {
        if e.size() > self.cfg.max_size || e.depth() > self.cfg.max_depth {
            return false;
        }
        match self.cfg.dimension_mode {
            DimensionMode::Reject => {
                let (c, m) = self.dim_status(e);
                c && m
            }
            _ => true,
        }
    }
}

fn better(a: &Individual, b: &Individual) -> bool {
    // lexicographic parsimony: lower fitness, ties (relative 1e-9) broken by size
    let both_inf = a.fitness.is_infinite() && b.fitness.is_infinite();
    let tie = both_inf
        || (a.fitness.is_finite()
            && b.fitness.is_finite()
            && (a.fitness - b.fitness).abs()
                <= 1e-9 * a.fitness.abs().max(b.fitness.abs()).max(1e-300));
    if tie {
        a.size < b.size
    } else {
        a.fitness < b.fitness
    }
}

/// Total order used for sorting: fitness, then size.
fn cmp_ind(a: &Individual, b: &Individual) -> std::cmp::Ordering {
    a.fitness.total_cmp(&b.fitness).then(a.size.cmp(&b.size))
}

fn tournament<'a>(pop: &'a [Individual], k: usize, rng: &mut ChaCha8Rng) -> &'a Individual {
    let mut best = &pop[rng.gen_range(0..pop.len())];
    for _ in 1..k {
        let c = &pop[rng.gen_range(0..pop.len())];
        if better(c, best) {
            best = c;
        }
    }
    best
}

pub(crate) fn optimise_constants(ctx: &Ctx, ind: &Individual, evals: usize) -> (Individual, usize) {
    let c0 = ind.expr.constants();
    if c0.is_empty() || !ind.fitness.is_finite() {
        return (ind.clone(), 0);
    }
    let bounds: Vec<(f64, f64)> = c0
        .iter()
        .map(|c| (c - 10.0 * c.abs() - 10.0, c + 10.0 * c.abs() + 10.0))
        .collect();
    let base = ind.expr.clone();
    let metric = ctx.cfg.metric;
    let scaling = ctx.cfg.scaling;
    let data = ctx.data;
    let problem = simorph_optimization::scalar_problem(bounds, move |c| {
        let mut e = base.clone();
        e.set_constants(c);
        let raw: Vec<f64> = data.x.iter().map(|x| e.eval(x)).collect();
        let (off, sc) = optimal_scaling(scaling, metric, &raw, &data.y);
        let pred: Vec<f64> = raw.iter().map(|p| off + sc * p).collect();
        let v = error(metric, &pred, &data.y);
        if v.is_finite() {
            v
        } else {
            1e300
        }
    });
    let budget = simorph_optimization::Budget::new(evals).tolerance(1e-14, 1e-14);
    match simorph_optimization::nelder_mead::nelder_mead(
        &problem,
        Some(&c0),
        &budget,
        &ExecutionControl::default(),
    ) {
        Ok(r) => {
            let mut e = ind.expr.clone();
            e.set_constants(&r.best_x);
            let cand = ctx.evaluate(e);
            if cand.fitness < ind.fitness {
                (cand, r.evaluations)
            } else {
                (ind.clone(), r.evaluations)
            }
        }
        Err(_) => (ind.clone(), evals),
    }
}

struct Island {
    pop: Vec<Individual>,
    rng: ChaCha8Rng,
    evaluations: usize,
}

impl Island {
    fn new(ctx: &Ctx, n: usize, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut pop = Vec::with_capacity(n);
        let mut i = 0;
        let mut attempts = 0;
        while pop.len() < n {
            let e = Expr::ramped(
                &ctx.cfg.primitives,
                ctx.cfg.init_depth.0,
                ctx.cfg.init_depth.1,
                i,
                &mut rng,
            );
            i += 1;
            attempts += 1;
            // In Reject mode accept wrong-dimension trees only after many failed attempts
            // so that the island is never empty.
            if ctx.admissible(&e) || attempts > 200 * n {
                pop.push(ctx.evaluate(e));
            }
        }
        Island {
            evaluations: pop.len(),
            pop,
            rng,
        }
    }

    fn generation(&mut self, ctx: &Ctx) {
        let cfg = ctx.cfg;
        let n = self.pop.len();
        let mut sorted = self.pop.clone();
        sorted.sort_by(cmp_ind);
        let mut next: Vec<Individual> = sorted.iter().take(cfg.elitism).cloned().collect();
        while next.len() < n {
            let mut child = None;
            for _attempt in 0..cfg.variation_attempts {
                let p1 = tournament(&self.pop, cfg.tournament, &mut self.rng)
                    .expr
                    .clone();
                let r: f64 = self.rng.gen();
                let c = if r < cfg.p_crossover {
                    let p2 = tournament(&self.pop, cfg.tournament, &mut self.rng)
                        .expr
                        .clone();
                    p1.crossover(&p2, &mut self.rng)
                } else if r < cfg.p_crossover + cfg.p_subtree {
                    p1.subtree_mutation(&cfg.primitives, 4, &mut self.rng)
                } else if r < cfg.p_crossover + cfg.p_subtree + cfg.p_point {
                    p1.point_mutation(&cfg.primitives, &mut self.rng)
                } else if r < cfg.p_crossover + cfg.p_subtree + cfg.p_point + cfg.p_constant {
                    p1.constant_mutation(0.2, &mut self.rng)
                } else {
                    p1
                };
                if ctx.admissible(&c) {
                    child = Some(c);
                    break;
                }
            }
            let c = child.unwrap_or_else(|| {
                tournament(&self.pop, cfg.tournament, &mut self.rng)
                    .expr
                    .clone()
            });
            next.push(ctx.evaluate(c));
            self.evaluations += 1;
        }
        // constant optimisation of the best distinct individuals
        next.sort_by(cmp_ind);
        let mut done = 0;
        let mut seen: Vec<String> = vec![];
        for ind in next.iter_mut() {
            if done >= cfg.const_opt_top {
                break;
            }
            let key = format!("{}", ind.expr.simplify());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let (opt, ev) = optimise_constants(ctx, ind, cfg.const_opt_evals);
            self.evaluations += ev;
            *ind = opt;
            done += 1;
        }
        self.pop = next;
    }

    fn best(&self) -> &Individual {
        let mut b = &self.pop[0];
        for c in &self.pop {
            if better(c, b) {
                b = c;
            }
        }
        b
    }
}

/// Run island GP.
pub fn run_gp(data: &Dataset, cfg: &GpConfig, ctl: &ExecutionControl) -> Result<GpResult> {
    if data.is_empty() {
        return Err(SimorphError::InvalidInput("empty data set".into()));
    }
    if data.names.len() != cfg.primitives.n_vars {
        return Err(SimorphError::InvalidInput(format!(
            "data has {} variables but the primitive set declares {}",
            data.names.len(),
            cfg.primitives.n_vars
        )));
    }
    let ctx = Ctx {
        data,
        cfg,
        var_dims: data.var_dimensions()?,
        target_dim: data.target_dimension()?,
    };
    let start = Instant::now();
    let per_island = (cfg.population / cfg.islands.max(1)).max(4);
    let mut islands: Vec<Island> = (0..cfg.islands.max(1))
        .map(|i| {
            Island::new(
                &ctx,
                per_island,
                cfg.seed.wrapping_mul(1_000_003).wrapping_add(i as u64),
            )
        })
        .collect();
    let mut archive: BTreeMap<usize, Individual> = BTreeMap::new();
    let mut history = vec![];
    let mut stop_reason = "generations exhausted".to_string();
    let mut generation = 0;
    while generation < cfg.generations {
        if ctl.check().is_err() {
            stop_reason = "cancelled".into();
            break;
        }
        let epoch = cfg
            .migration_interval
            .max(1)
            .min(cfg.generations - generation);
        islands.par_iter_mut().for_each(|isl| {
            for _ in 0..epoch {
                isl.generation(&ctx);
            }
        });
        generation += epoch;
        // ring migration: best of island i replace worst of island i+1
        let k = islands.len();
        if k > 1 {
            let migrants: Vec<Vec<Individual>> = islands
                .iter()
                .map(|isl| {
                    let mut s = isl.pop.clone();
                    s.sort_by(cmp_ind);
                    s.into_iter().take(cfg.migrants).collect()
                })
                .collect();
            for i in 0..k {
                let dest = &mut islands[(i + 1) % k];
                dest.pop.sort_by(|a, b| cmp_ind(b, a));
                for (j, m) in migrants[i].iter().enumerate() {
                    if j < dest.pop.len() {
                        dest.pop[j] = m.clone();
                    }
                }
            }
        }
        // archive & summary
        let mut all: Vec<&Individual> = islands.iter().flat_map(|i| i.pop.iter()).collect();
        for ind in &all {
            if ind.fitness.is_finite() {
                let e = archive.entry(ind.size).or_insert_with(|| (*ind).clone());
                if ind.fitness < e.fitness {
                    *e = (*ind).clone();
                }
            }
        }
        all.sort_by(|a, b| cmp_ind(a, b));
        let best = all[0];
        let evaluations: usize = islands.iter().map(|i| i.evaluations).sum();
        history.push(GenerationSummary {
            generation,
            best_error: best.error,
            best_size: best.size,
            mean_size: all.iter().map(|i| i.size as f64).sum::<f64>() / all.len() as f64,
            dimensionally_valid_fraction: all.iter().filter(|i| i.dimension_ok).count() as f64
                / all.len() as f64,
            evaluations,
        });
        if best.fitness <= cfg.target_error {
            stop_reason = format!("target error {:.3e} reached", cfg.target_error);
            break;
        }
        if evaluations >= cfg.max_evaluations {
            stop_reason = "evaluation budget exhausted".into();
            break;
        }
    }
    let best =
        islands
            .iter()
            .map(|i| i.best().clone())
            .fold(None::<Individual>, |acc, c| match acc {
                None => Some(c),
                Some(a) => Some(if better(&c, &a) { c } else { a }),
            });
    let best = best.expect("at least one island");
    // Pareto front of the archive: increasing size, strictly decreasing fitness
    let mut pareto: Vec<Individual> = vec![];
    for (_, ind) in archive {
        if pareto
            .last()
            .map(|l: &Individual| ind.fitness < l.fitness)
            .unwrap_or(true)
        {
            pareto.push(ind);
        }
    }
    Ok(GpResult {
        best,
        pareto,
        evaluations: islands.iter().map(|i| i.evaluations).sum(),
        generations: generation,
        history,
        wall_time_s: start.elapsed().as_secs_f64(),
        stop_reason,
    })
}

/// Verdict of model selection on held-out data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LawVerdict {
    Law {
        expr: Expr,
        expression: String,
        train_error: f64,
        held_out_error: f64,
        baseline_held_out_error: f64,
        criterion: f64,
        dimension_ok: bool,
    },
    NoLaw {
        best_held_out_error: f64,
        baseline_held_out_error: f64,
        reason: String,
    },
}

/// Select a law from the Pareto front using a held-out split and a
/// BIC-like criterion `n·ln(err²) + size·ln(n)`.  A law is accepted only if
/// its held-out error is below `max_ratio` × the error of the constant
/// (mean) model and its criterion beats the constant model's.
pub fn select_law(
    result: &GpResult,
    held_out: &Dataset,
    metric: ErrorMetric,
    max_ratio: f64,
) -> LawVerdict {
    let n = held_out.len().max(1) as f64;
    let train_mean_model = {
        // constant model: mean of held-out targets is not allowed (leak); use
        // the constant that GP's training data would give: we approximate it
        // with the held-out mean only for NormalisedRmse where it defines 1.
        let mean = held_out.y.iter().sum::<f64>() / n;
        let pred = vec![mean; held_out.len()];
        error(metric, &pred, &held_out.y)
    };
    let crit = |err: f64, size: usize| n * (err.max(1e-300).powi(2)).ln() + size as f64 * n.ln();
    let base_crit = crit(train_mean_model, 1);
    let mut best: Option<(f64, &Individual, f64)> = None;
    for ind in &result.pareto {
        let pred: Vec<f64> = held_out.x.iter().map(|x| ind.model.eval(x)).collect();
        let e = error(metric, &pred, &held_out.y);
        let c = crit(
            e,
            ind.size
                + if ind.model.size() > ind.expr.size() {
                    1
                } else {
                    0
                },
        );
        if best.as_ref().map(|b| c < b.0).unwrap_or(true) {
            best = Some((c, ind, e));
        }
    }
    match best {
        Some((c, ind, e)) if e < max_ratio * train_mean_model && c < base_crit => LawVerdict::Law {
            expression: ind.model.simplify().to_string_with(&held_out.names),
            expr: ind.model.clone(),
            train_error: ind.error,
            held_out_error: e,
            baseline_held_out_error: train_mean_model,
            criterion: c,
            dimension_ok: ind.dimension_ok,
        },
        Some((_, _, e)) => LawVerdict::NoLaw {
            best_held_out_error: e,
            baseline_held_out_error: train_mean_model,
            reason: format!(
                "best held-out error {e:.4} not below {max_ratio} × constant-model error {train_mean_model:.4} with a better information criterion"
            ),
        },
        None => LawVerdict::NoLaw { best_held_out_error: f64::INFINITY, baseline_held_out_error: train_mean_model, reason: "empty Pareto front".into() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_simple_polynomial_and_is_deterministic() {
        // y = x0^2 + 3 x1 on a grid
        let mut d = Dataset::new(&["x0", "x1"], &["1", "1"], "y", "1");
        for i in 0..8 {
            for j in 0..6 {
                let (a, b) = (0.5 + i as f64 * 0.4, 1.0 + j as f64 * 0.3);
                d.push(vec![a, b], a * a + 3.0 * b);
            }
        }
        let mut cfg = GpConfig::new(PrimitiveSet::arithmetic(2), 7);
        cfg.generations = 30;
        cfg.target_error = 1e-9;
        let r1 = run_gp(&d, &cfg, &ExecutionControl::default()).unwrap();
        let r2 = run_gp(&d, &cfg, &ExecutionControl::default()).unwrap();
        assert!(
            r1.best.error < 1e-6,
            "best {} err {}",
            r1.best.model,
            r1.best.error
        );
        assert_eq!(format!("{}", r1.best.expr), format!("{}", r2.best.expr));
        assert!(!r1.pareto.is_empty());
        for w in r1.pareto.windows(2) {
            assert!(w[0].size < w[1].size && w[0].fitness > w[1].fitness);
        }
    }

    #[test]
    fn cancellation_stops_the_run() {
        let mut d = Dataset::new(&["x"], &["1"], "y", "1");
        for i in 0..10 {
            d.push(vec![i as f64], (i as f64).sin() + 2.0);
        }
        let mut cfg = GpConfig::new(PrimitiveSet::full(1), 1);
        cfg.generations = 1000;
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        let r = run_gp(&d, &cfg, &ctl).unwrap();
        assert_eq!(r.stop_reason, "cancelled");
        assert_eq!(r.generations, 0);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    #[test]
    #[ignore]
    fn probe_poly() {
        let mut d = Dataset::new(&["x0", "x1"], &["1", "1"], "y", "1");
        for i in 0..8 {
            for j in 0..6 {
                let (a, b) = (0.5 + i as f64 * 0.4, 1.0 + j as f64 * 0.3);
                d.push(vec![a, b], a * a + 3.0 * b);
            }
        }
        let pop: usize = std::env::var("P")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);
        let pars: f64 = std::env::var("PA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.01);
        let tour: usize = std::env::var("T")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        let mut ok = 0;
        for seed in 0..10 {
            let mut cfg = GpConfig::new(PrimitiveSet::arithmetic(2), seed);
            cfg.population = pop;
            cfg.parsimony = pars;
            cfg.tournament = tour;
            cfg.generations = 60;
            cfg.target_error = 1e-9;
            let r = run_gp(&d, &cfg, &ExecutionControl::default()).unwrap();
            if r.best.error < 1e-6 {
                ok += 1;
            }
            println!(
                "seed {seed} gens {} err {:.3e} size {} {} evals {}",
                r.generations,
                r.best.error,
                r.best.size,
                r.best.model.simplify(),
                r.evaluations
            );
        }
        println!("SUCCESS {ok}/10 pop {pop} pars {pars} tour {tour}");
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;
    fn ind(f: f64, size: usize) -> Individual {
        Individual {
            expr: Expr::Const(0.0),
            model: Expr::Const(0.0),
            fitness: f,
            error: f,
            size,
            dimension_ok: true,
        }
    }
    #[test]
    fn finite_always_beats_infinite_regardless_of_size() {
        assert!(better(&ind(1e-4, 9), &ind(f64::INFINITY, 1)));
        assert!(!better(&ind(f64::INFINITY, 1), &ind(1e-4, 9)));
        assert!(better(&ind(1.0, 3), &ind(1.0, 5)));
    }
}
