//! Logistic-regression classifier on a deterministic synthetic dataset.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use simorph_core::*;

/// Deterministic two-class dataset: points in the plane with a nonlinear
/// boundary; features are `[1, x, y, x*y, x², y²]` so a linear model can fit it.
pub fn dataset(seed: u64, n: usize) -> (Vec<[f64; 6]>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    for _ in 0..n {
        let x: f64 = rng.gen_range(-2.0..2.0);
        let y: f64 = rng.gen_range(-2.0..2.0);
        let noise: f64 = rng.gen_range(-0.3..0.3);
        let label = if x * x + y * y - 2.0 + 0.5 * x * y + noise > 0.0 {
            1.0
        } else {
            0.0
        };
        xs.push([1.0, x, y, x * y, x * x, y * y]);
        ys.push(label);
    }
    (xs, ys)
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

pub fn evaluate(weights: &[f64], xs: &[[f64; 6]], ys: &[f64]) -> (f64, f64) {
    let mut correct = 0usize;
    let mut loss = 0.0;
    for (x, &y) in xs.iter().zip(ys) {
        let z: f64 = x.iter().zip(weights).map(|(a, b)| a * b).sum();
        let p = sigmoid(z).clamp(1e-12, 1.0 - 1e-12);
        if (p >= 0.5) == (y >= 0.5) {
            correct += 1;
        }
        loss -= y * p.ln() + (1.0 - y) * (1.0 - p).ln();
    }
    (correct as f64 / xs.len() as f64, loss / xs.len() as f64)
}

/// Evaluation component: candidate weights → accuracy, log-loss, latency.
pub struct ClassifierEval {
    m: ComponentManifest,
}

impl ClassifierEval {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("ai.classifier.evaluate", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::Evaluation, Capability::Inference])
            .with_domains(&["ai", "machine-learning"])
            .with_inputs(vec![
                Port { name: "weights".into(), unit: "1".into(), description: Some("6 weights".into()), shape: vec![6], range: None },
                Port::scalar("dataset_seed", "1"),
                Port::scalar("n_samples", "1").with_range(10.0, 1e6),
            ])
            .with_outputs(vec![Port::scalar("accuracy", "1"), Port::scalar("log_loss", "1")])
            .with_assumptions(&["synthetic dataset generated from the seed; the held-out set must use a different seed than training"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "zero weights give chance-level log-loss ln 2".into(),
                    inputs: json!({"weights": [0,0,0,0,0,0], "dataset_seed": 1, "n_samples": 1000}),
                    expected_outputs: json!({"log_loss": std::f64::consts::LN_2}),
                    rtol: 1e-9,
                    atol: 0.0,
                    source: "definition of binary cross-entropy at p=0.5".into(),
                }],
                invariants: vec!["accuracy >= 0".into(), "accuracy <= 1".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for ClassifierEval {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for ClassifierEval {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let w = req.get_vec_f64("weights")?;
        if w.len() != 6 {
            return Err(SimorphError::InvalidInput(
                "weights must have 6 entries".into(),
            ));
        }
        // The request seed selects an independent sample of the same
        // distribution, so repeated evaluations with different seeds are
        // genuine replications (not identical repeats).
        let seed = (req.get_f64("dataset_seed")? as u64)
            ^ req.seed.unwrap_or(0).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let n = req.get_f64("n_samples")? as usize;
        ctl.check()?;
        let (xs, ys) = dataset(seed, n);
        let t0 = std::time::Instant::now();
        let (acc, loss) = evaluate(&w, &xs, &ys);
        let lat = t0.elapsed().as_secs_f64() / n as f64;
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("accuracy", acc)
            .output("log_loss", loss)
            .metric("latency_per_sample", lat))
    }
}

/// Training component: gradient descent from initial weights.
pub struct ClassifierTrain {
    m: ComponentManifest,
}

impl ClassifierTrain {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("ai.classifier.train", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::Training])
            .with_domains(&["ai", "machine-learning"])
            .with_inputs(vec![
                Port {
                    name: "weights".into(),
                    unit: "1".into(),
                    description: None,
                    shape: vec![6],
                    range: None,
                },
                Port::scalar("dataset_seed", "1"),
                Port::scalar("n_samples", "1").with_range(10.0, 1e6),
                Port::scalar("learning_rate", "1").with_range(1e-6, 10.0),
                Port::scalar("epochs", "1").with_range(0.0, 1e5),
            ])
            .with_outputs(vec![
                Port {
                    name: "weights".into(),
                    unit: "1".into(),
                    description: None,
                    shape: vec![6],
                    range: None,
                },
                Port::scalar("train_log_loss", "1"),
            ])
            .with_assumptions(&["full-batch gradient descent, no regularisation"]);
        Self { m }
    }
}

impl Default for ClassifierTrain {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for ClassifierTrain {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let mut w = req.get_vec_f64("weights")?;
        if w.len() != 6 {
            return Err(SimorphError::InvalidInput(
                "weights must have 6 entries".into(),
            ));
        }
        let seed = req.get_f64("dataset_seed")? as u64;
        let n = req.get_f64("n_samples")? as usize;
        let lr = req.get_f64("learning_rate")?;
        let epochs = req.get_f64("epochs")? as usize;
        let (xs, ys) = dataset(seed, n);
        for e in 0..epochs {
            if e % 50 == 0 {
                ctl.check()?;
            }
            let mut grad = [0.0; 6];
            for (x, &y) in xs.iter().zip(&ys) {
                let z: f64 = x.iter().zip(&w).map(|(a, b)| a * b).sum();
                let p = sigmoid(z);
                for k in 0..6 {
                    grad[k] += (p - y) * x[k];
                }
            }
            for k in 0..6 {
                w[k] -= lr * grad[k] / n as f64;
            }
        }
        let (_, loss) = evaluate(&w, &xs, &ys);
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("weights", json!(w))
            .output("train_log_loss", loss))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn training_improves_held_out_accuracy() {
        let ev = ClassifierEval::new();
        let tr = ClassifierTrain::new();
        let ctl = ExecutionControl::default();
        let zero = json!([0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let before = run_component(
            &ev,
            &TaskRequest::new(ev.m.id)
                .input("weights", zero.clone())
                .input("dataset_seed", 99)
                .input("n_samples", 2000),
            &ctl,
        )
        .unwrap();
        assert!((before.get_f64("log_loss").unwrap() - std::f64::consts::LN_2).abs() < 1e-9);
        let trained = run_component(
            &tr,
            &TaskRequest::new(tr.m.id)
                .input("weights", zero)
                .input("dataset_seed", 1)
                .input("n_samples", 2000)
                .input("learning_rate", 0.5)
                .input("epochs", 300),
            &ctl,
        )
        .unwrap();
        let w = trained.outputs["weights"].clone();
        let after = run_component(
            &ev,
            &TaskRequest::new(ev.m.id)
                .input("weights", w)
                .input("dataset_seed", 99)
                .input("n_samples", 2000),
            &ctl,
        )
        .unwrap();
        assert!(
            after.get_f64("accuracy").unwrap() > 0.85,
            "{}",
            after.get_f64("accuracy").unwrap()
        );
        assert!(after.get_f64("log_loss").unwrap() < before.get_f64("log_loss").unwrap());
    }
}
