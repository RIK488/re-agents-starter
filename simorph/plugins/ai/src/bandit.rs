//! ε-greedy agent on a fixed Bernoulli multi-armed bandit.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use simorph_core::*;

pub struct BanditAgent {
    m: ComponentManifest,
}

impl BanditAgent {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("ai.agent.epsilon_greedy_bandit", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::Evaluation, Capability::MultiAgent])
            .with_domains(&["ai", "reinforcement-learning", "decision-making"])
            .with_inputs(vec![
                Port::scalar("epsilon", "1").with_range(0.0, 1.0),
                Port::scalar("optimistic_init", "1").with_range(0.0, 1.0),
                Port { name: "arm_probabilities".into(), unit: "1".into(), description: None, shape: vec![0], range: None },
                Port::scalar("horizon", "1").with_range(1.0, 1e6),
                Port::scalar("episodes", "1").with_range(1.0, 1e4),
            ])
            .with_outputs(vec![Port::scalar("mean_regret", "1"), Port::scalar("mean_reward", "1")])
            .with_assumptions(&["Bernoulli arms with stationary probabilities; sample-average value estimates"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "single arm has zero regret".into(),
                    inputs: json!({"epsilon": 0.1, "optimistic_init": 0.0, "arm_probabilities": [0.5], "horizon": 100, "episodes": 5}),
                    expected_outputs: json!({"mean_regret": 0.0}),
                    rtol: 0.0,
                    atol: 1e-12,
                    source: "definition of regret".into(),
                }],
                invariants: vec!["mean_regret >= 0".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for BanditAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for BanditAgent {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let eps = req.get_f64("epsilon")?;
        let init = req.get_f64("optimistic_init")?;
        let probs = req.get_vec_f64("arm_probabilities")?;
        let horizon = req.get_f64("horizon")? as usize;
        let episodes = req.get_f64("episodes")? as usize;
        if probs.is_empty() {
            return Err(SimorphError::InvalidInput("arm_probabilities empty".into()));
        }
        let best = probs.iter().cloned().fold(f64::MIN, f64::max);
        let mut rng = ChaCha8Rng::seed_from_u64(req.seed.unwrap_or(0));
        let mut total_regret = 0.0;
        let mut total_reward = 0.0;
        for _ in 0..episodes {
            ctl.check()?;
            let mut q = vec![init; probs.len()];
            let mut n = vec![0usize; probs.len()];
            for _ in 0..horizon {
                let arm = if rng.gen::<f64>() < eps {
                    rng.gen_range(0..probs.len())
                } else {
                    let mut b = 0;
                    for i in 1..probs.len() {
                        if q[i] > q[b] {
                            b = i;
                        }
                    }
                    b
                };
                let r = if rng.gen::<f64>() < probs[arm] { 1.0 } else { 0.0 };
                n[arm] += 1;
                q[arm] += (r - q[arm]) / n[arm] as f64;
                total_reward += r;
                total_regret += best - probs[arm];
            }
        }
        let denom = (episodes * horizon) as f64;
        Ok(TaskResult::new(req, EvidenceLevel::Simulated)
            .output("mean_regret", total_regret / denom)
            .output("mean_reward", total_reward / denom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderate_exploration_beats_pure_exploitation_without_optimism() {
        let c = BanditAgent::new();
        let ctl = ExecutionControl::default();
        let run = |eps: f64| {
            run_component(&c, &TaskRequest::new(c.m.id).seed(42).input("epsilon", eps).input("optimistic_init", 0.0).input("arm_probabilities", json!([0.2, 0.5, 0.8])).input("horizon", 500).input("episodes", 40), &ctl).unwrap().get_f64("mean_regret").unwrap()
        };
        let greedy = run(0.0);
        let explore = run(0.1);
        assert!(explore < greedy, "explore {explore} greedy {greedy}");
        // determinism
        assert_eq!(run(0.1), run(0.1));
    }
}
