//! Constraint handling: penalty wrapper and Deb's feasibility rules.

use crate::problem::*;
use simorph_core::Result;
use std::cmp::Ordering;

/// Wraps a constrained problem into an unconstrained one with a quadratic
/// penalty, for methods that have no native constraint support.
pub struct PenaltyProblem<'a> {
    pub inner: &'a dyn ObjectiveFunction,
    pub penalty: f64,
}

impl<'a> ObjectiveFunction for PenaltyProblem<'a> {
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
    fn bounds(&self) -> Vec<(f64, f64)> {
        self.inner.bounds()
    }
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
        let e = self.inner.evaluate(x)?;
        Ok(Evaluation {
            objectives: vec![scalarize(&e, self.penalty)],
            constraints: vec![],
            cost: e.cost,
        })
    }
}

/// Deb's feasibility rules: feasible beats infeasible; among feasible the
/// lower objective wins; among infeasible the lower violation wins.
pub fn deb_compare(a: &Evaluation, b: &Evaluation) -> Ordering {
    let (fa, fb) = (a.is_feasible(), b.is_feasible());
    match (fa, fb) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.objectives[0].total_cmp(&b.objectives[0]),
        (false, false) => a.violation().total_cmp(&b.violation()),
    }
}

/// Scalar "fitness" consistent with Deb's rules, usable for sorting: feasible
/// points keep their objective, infeasible ones are ranked above the worst
/// feasible objective by their violation.
pub fn deb_key(e: &Evaluation, worst_feasible: f64) -> f64 {
    if e.is_feasible() {
        e.objectives[0]
    } else {
        worst_feasible + e.violation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feasible_beats_infeasible() {
        let a = Evaluation::scalar(10.0).with_constraints(vec![-1.0]);
        let b = Evaluation::scalar(-100.0).with_constraints(vec![0.5]);
        assert_eq!(deb_compare(&a, &b), Ordering::Less);
    }
}
