//! Common optimisation interfaces.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result of one objective evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    /// Objective values (all to be minimised; adapters negate maximisation).
    pub objectives: Vec<f64>,
    /// Constraint values g_i(x); feasible iff all g_i <= 0.
    pub constraints: Vec<f64>,
    /// Evaluation cost (seconds or arbitrary units).
    pub cost: f64,
}

impl Evaluation {
    pub fn scalar(f: f64) -> Self {
        Self {
            objectives: vec![f],
            constraints: vec![],
            cost: 0.0,
        }
    }
    pub fn with_constraints(mut self, g: Vec<f64>) -> Self {
        self.constraints = g;
        self
    }
    pub fn is_feasible(&self) -> bool {
        self.constraints.iter().all(|&g| g <= 0.0)
    }
    /// Sum of positive constraint violations.
    pub fn violation(&self) -> f64 {
        self.constraints.iter().map(|&g| g.max(0.0)).sum()
    }
}

/// An optimisation problem.
pub trait ObjectiveFunction: Sync {
    fn dimension(&self) -> usize;
    /// Lower and upper bounds per variable.
    fn bounds(&self) -> Vec<(f64, f64)>;
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation>;
    fn n_objectives(&self) -> usize {
        1
    }
    fn n_constraints(&self) -> usize {
        0
    }
    /// Optional analytic gradient of the first objective.
    fn gradient(&self, _x: &[f64]) -> Option<Vec<f64>> {
        None
    }
    /// Variables that must be integer (indices).
    fn integer_variables(&self) -> Vec<usize> {
        vec![]
    }
}

/// Build a problem from closures.
pub struct FnProblem<F: Fn(&[f64]) -> Result<Evaluation> + Sync> {
    pub bounds: Vec<(f64, f64)>,
    pub f: F,
    pub n_objectives: usize,
    pub n_constraints: usize,
}

impl<F: Fn(&[f64]) -> Result<Evaluation> + Sync> FnProblem<F> {
    pub fn new(bounds: Vec<(f64, f64)>, f: F) -> Self {
        Self {
            bounds,
            f,
            n_objectives: 1,
            n_constraints: 0,
        }
    }
    pub fn objectives(mut self, n: usize) -> Self {
        self.n_objectives = n;
        self
    }
    pub fn constraints(mut self, n: usize) -> Self {
        self.n_constraints = n;
        self
    }
}

impl<F: Fn(&[f64]) -> Result<Evaluation> + Sync> ObjectiveFunction for FnProblem<F> {
    fn dimension(&self) -> usize {
        self.bounds.len()
    }
    fn bounds(&self) -> Vec<(f64, f64)> {
        self.bounds.clone()
    }
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
        (self.f)(x)
    }
    fn n_objectives(&self) -> usize {
        self.n_objectives
    }
    fn n_constraints(&self) -> usize {
        self.n_constraints
    }
}

/// Scalar problem from a plain function (no constraints).
pub fn scalar_problem<F: Fn(&[f64]) -> f64 + Sync>(
    bounds: Vec<(f64, f64)>,
    f: F,
) -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    FnProblem::new(bounds, move |x| Ok(Evaluation::scalar(f(x))))
}

/// Wrapper counting evaluations and recording the best point so far.
pub struct Counted<'a> {
    pub inner: &'a dyn ObjectiveFunction,
    count: AtomicUsize,
}

impl<'a> Counted<'a> {
    pub fn new(inner: &'a dyn ObjectiveFunction) -> Self {
        Self {
            inner,
            count: AtomicUsize::new(0),
        }
    }
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
    pub fn eval(&self, x: &[f64]) -> Result<Evaluation> {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.inner.evaluate(x)
    }
    /// Scalarised objective (first objective + penalty when constrained).
    pub fn eval_scalar(&self, x: &[f64], penalty: f64) -> Result<f64> {
        let e = self.eval(x)?;
        Ok(scalarize(&e, penalty))
    }
}

/// First objective plus quadratic penalty on violations.
pub fn scalarize(e: &Evaluation, penalty: f64) -> f64 {
    let f = e.objectives.first().copied().unwrap_or(f64::INFINITY);
    let v: f64 = e.constraints.iter().map(|&g| g.max(0.0).powi(2)).sum();
    f + penalty * v
}

/// Optimisation method identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    NelderMead,
    GradientDescent,
    Lbfgs,
    DifferentialEvolution,
    CmaEs,
    SimulatedAnnealing,
    Nsga2,
    BayesianOptimization,
    MonteCarloTreeSearch,
    BeamSearch,
    RobustExpectation,
    RobustCvar,
    QLearning,
    RandomSearch,
}

/// A step of the optimisation history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub evaluations: usize,
    pub best_f: f64,
}

/// Result of a single-objective optimisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub method: Method,
    pub best_x: Vec<f64>,
    /// Best (scalarised) objective value.
    pub best_f: f64,
    /// Raw evaluation at `best_x`.
    pub best_eval: Evaluation,
    pub evaluations: usize,
    pub iterations: usize,
    pub converged: bool,
    pub history: Vec<HistoryPoint>,
    pub message: String,
}

/// Common stopping options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub max_evaluations: usize,
    pub max_iterations: usize,
    /// Stop when |f_best| improvement over a window is below this.
    pub f_tol: f64,
    pub x_tol: f64,
    pub seed: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_evaluations: 10_000,
            max_iterations: 1_000,
            f_tol: 1e-10,
            x_tol: 1e-10,
            seed: 0,
        }
    }
}

impl Budget {
    pub fn new(max_evaluations: usize) -> Self {
        Self {
            max_evaluations,
            ..Default::default()
        }
    }
    pub fn seed(mut self, s: u64) -> Self {
        self.seed = s;
        self
    }
    pub fn iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }
    pub fn tolerance(mut self, f_tol: f64, x_tol: f64) -> Self {
        self.f_tol = f_tol;
        self.x_tol = x_tol;
        self
    }
}

pub(crate) fn clamp_to_bounds(x: &mut [f64], bounds: &[(f64, f64)]) {
    for (xi, (lo, hi)) in x.iter_mut().zip(bounds) {
        if *xi < *lo {
            *xi = *lo;
        }
        if *xi > *hi {
            *xi = *hi;
        }
    }
}

pub(crate) fn check_dim(p: &dyn ObjectiveFunction) -> Result<usize> {
    let d = p.dimension();
    if d == 0 || p.bounds().len() != d {
        return Err(SimorphError::InvalidInput(
            "problem dimension/bounds mismatch".into(),
        ));
    }
    for (lo, hi) in p.bounds() {
        if !(lo <= hi) {
            return Err(SimorphError::InvalidInput("bounds low > high".into()));
        }
    }
    Ok(d)
}

/// Uniform random point in bounds.
pub(crate) fn random_point<R: rand::Rng>(bounds: &[(f64, f64)], rng: &mut R) -> Vec<f64> {
    bounds
        .iter()
        .map(|&(lo, hi)| if hi > lo { rng.gen_range(lo..hi) } else { lo })
        .collect()
}

/// Random search baseline (used by tests and as a fallback).
pub fn random_search(
    p: &dyn ObjectiveFunction,
    budget: &Budget,
    ctl: &simorph_core::ExecutionControl,
) -> Result<OptimizationResult> {
    check_dim(p)?;
    let bounds = p.bounds();
    let mut rng = simorph_uncertainty::seeded(budget.seed);
    let c = Counted::new(p);
    let mut best_x = vec![];
    let mut best_f = f64::INFINITY;
    let mut best_eval = Evaluation::scalar(f64::INFINITY);
    let mut history = vec![];
    while c.count() < budget.max_evaluations {
        ctl.check()?;
        let x = random_point(&bounds, &mut rng);
        let e = c.eval(&x)?;
        let f = scalarize(&e, 1e6);
        if f < best_f {
            best_f = f;
            best_x = x;
            best_eval = e;
            history.push(HistoryPoint {
                evaluations: c.count(),
                best_f,
            });
        }
    }
    Ok(OptimizationResult {
        method: Method::RandomSearch,
        best_x,
        best_f,
        best_eval,
        evaluations: c.count(),
        iterations: budget.max_evaluations,
        converged: false,
        history,
        message: "budget exhausted".into(),
    })
}
