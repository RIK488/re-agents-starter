//! Standard benchmark functions (all minimisation).

use crate::problem::{scalar_problem, Evaluation, FnProblem};
use simorph_core::Result;

pub fn sphere(x: &[f64]) -> f64 {
    x.iter().map(|v| v * v).sum()
}

pub fn rosenbrock(x: &[f64]) -> f64 {
    x.windows(2)
        .map(|w| 100.0 * (w[1] - w[0] * w[0]).powi(2) + (1.0 - w[0]).powi(2))
        .sum()
}

pub fn rastrigin(x: &[f64]) -> f64 {
    10.0 * x.len() as f64
        + x.iter()
            .map(|v| v * v - 10.0 * (2.0 * std::f64::consts::PI * v).cos())
            .sum::<f64>()
}

pub fn sphere_problem(d: usize) -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    scalar_problem(vec![(-5.0, 5.0); d], sphere)
}
pub fn rosenbrock_problem(d: usize) -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    scalar_problem(vec![(-2.0, 2.0); d], rosenbrock)
}
pub fn rastrigin_problem(d: usize) -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    scalar_problem(vec![(-5.12, 5.12); d], rastrigin)
}

/// ZDT1 bi-objective benchmark, Pareto front f2 = 1 - sqrt(f1), f1 in [0,1].
pub fn zdt1(x: &[f64]) -> Vec<f64> {
    let n = x.len() as f64;
    let f1 = x[0];
    let g = 1.0 + 9.0 * x[1..].iter().sum::<f64>() / (n - 1.0);
    let f2 = g * (1.0 - (f1 / g).sqrt());
    vec![f1, f2]
}

pub fn zdt1_problem(d: usize) -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    FnProblem::new(vec![(0.0, 1.0); d], |x| {
        Ok(Evaluation {
            objectives: zdt1(x),
            constraints: vec![],
            cost: 0.0,
        })
    })
    .objectives(2)
}

/// Constrained problem g06 (Floudas & Pardalos):
/// min (x1-10)^3 + (x2-20)^3, s.t. -(x1-5)^2 - (x2-5)^2 + 100 <= 0,
/// (x1-6)^2 + (x2-5)^2 - 82.81 <= 0, 13 <= x1 <= 100, 0 <= x2 <= 100.
/// Optimum f* = -6961.81388 at (14.095, 0.84296).
pub fn g06_problem() -> FnProblem<impl Fn(&[f64]) -> Result<Evaluation> + Sync> {
    FnProblem::new(vec![(13.0, 100.0), (0.0, 100.0)], |x| {
        Ok(Evaluation {
            objectives: vec![(x[0] - 10.0).powi(3) + (x[1] - 20.0).powi(3)],
            constraints: vec![
                -(x[0] - 5.0).powi(2) - (x[1] - 5.0).powi(2) + 100.0,
                (x[0] - 6.0).powi(2) + (x[1] - 5.0).powi(2) - 82.81,
            ],
            cost: 0.0,
        })
    })
    .constraints(2)
}

pub const G06_OPTIMUM: f64 = -6961.81388;
