//! Bounded-depth arithmetic reasoner.
//!
//! Tasks: evaluate nested integer expressions such as `((3+4)*(2-7))+(6*5)`
//! generated deterministically from a seed with a given nesting depth.  The
//! candidate parameter `max_depth` bounds how deep the reasoner recurses: a
//! sub-expression deeper than `max_depth` is *approximated* by a heuristic
//! (returns 0), which is fast but wrong.  This models the real trade-off
//! between inference speed and reasoning quality and gives the AI laboratory
//! a test case where "faster" must not be accepted as "better".

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use simorph_core::*;

#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn random(rng: &mut ChaCha8Rng, depth: usize) -> Expr {
        if depth == 0 {
            return Expr::Num(rng.gen_range(-9..=9));
        }
        let a = Box::new(Expr::random(rng, depth - 1));
        let b = Box::new(Expr::random(rng, depth - 1));
        match rng.gen_range(0..3) {
            0 => Expr::Add(a, b),
            1 => Expr::Sub(a, b),
            _ => Expr::Mul(a, b),
        }
    }
    pub fn exact(&self) -> i64 {
        match self {
            Expr::Num(n) => *n,
            Expr::Add(a, b) => a.exact().wrapping_add(b.exact()),
            Expr::Sub(a, b) => a.exact().wrapping_sub(b.exact()),
            Expr::Mul(a, b) => a.exact().wrapping_mul(b.exact()),
        }
    }
    /// Bounded reasoning: beyond `budget` levels the sub-expression is guessed as 0.
    pub fn bounded(&self, budget: usize, steps: &mut u64) -> i64 {
        *steps += 1;
        match self {
            Expr::Num(n) => *n,
            _ if budget == 0 => 0,
            Expr::Add(a, b) => a
                .bounded(budget - 1, steps)
                .wrapping_add(b.bounded(budget - 1, steps)),
            Expr::Sub(a, b) => a
                .bounded(budget - 1, steps)
                .wrapping_sub(b.bounded(budget - 1, steps)),
            Expr::Mul(a, b) => a
                .bounded(budget - 1, steps)
                .wrapping_mul(b.bounded(budget - 1, steps)),
        }
    }
}

pub struct ArithmeticReasoner {
    m: ComponentManifest,
}

impl ArithmeticReasoner {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("ai.reasoner.arithmetic", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::Evaluation, Capability::Inference])
            .with_domains(&["ai", "reasoning"])
            .with_inputs(vec![
                Port::scalar("max_depth", "1").with_range(0.0, 64.0),
                Port::scalar("task_depth", "1").with_range(0.0, 12.0),
                Port::scalar("n_tasks", "1").with_range(1.0, 1e5),
                Port::scalar("task_seed", "1"),
            ])
            .with_outputs(vec![
                Port::scalar("accuracy", "1"),
                Port::scalar("mean_steps", "1"),
            ])
            .with_assumptions(&["tasks are integer expression trees of uniform depth; a task is solved only if the exact value is produced"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "full depth gives perfect accuracy".into(),
                    inputs: json!({"max_depth": 6, "task_depth": 4, "n_tasks": 200, "task_seed": 7}),
                    expected_outputs: json!({"accuracy": 1.0}),
                    rtol: 0.0,
                    atol: 0.0,
                    source: "exact evaluation".into(),
                }],
                invariants: vec!["accuracy <= 1".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for ArithmeticReasoner {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for ArithmeticReasoner {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let max_depth = req.get_f64("max_depth")? as usize;
        let task_depth = req.get_f64("task_depth")? as usize;
        let n = req.get_f64("n_tasks")? as usize;
        let seed = req.get_f64("task_seed")? as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let tasks: Vec<Expr> = (0..n).map(|_| Expr::random(&mut rng, task_depth)).collect();
        ctl.check()?;
        let t0 = std::time::Instant::now();
        let mut correct = 0usize;
        let mut steps = 0u64;
        for t in &tasks {
            if t.bounded(max_depth, &mut steps) == t.exact() {
                correct += 1;
            }
        }
        let lat = t0.elapsed().as_secs_f64() / n as f64;
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("accuracy", correct as f64 / n as f64)
            .output("mean_steps", steps as f64 / n as f64)
            .metric("latency_per_task", lat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_trades_accuracy_for_steps() {
        let c = ArithmeticReasoner::new();
        let ctl = ExecutionControl::default();
        let run = |d: f64| {
            run_component(
                &c,
                &TaskRequest::new(c.m.id)
                    .input("max_depth", d)
                    .input("task_depth", 5)
                    .input("n_tasks", 300)
                    .input("task_seed", 3),
                &ctl,
            )
            .unwrap()
        };
        let full = run(6.0);
        let shallow = run(2.0);
        assert_eq!(full.get_f64("accuracy").unwrap(), 1.0);
        assert!(shallow.get_f64("accuracy").unwrap() < 0.5);
        assert!(shallow.get_f64("mean_steps").unwrap() < full.get_f64("mean_steps").unwrap());
    }
}
