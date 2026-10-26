//! # simorph-plugin-ai
//!
//! Components that *are* AI systems, so that the AI laboratory (§5) has real,
//! measurable improvement targets whose behaviour is fully reproducible:
//!
//! * [`classifier`] – logistic-regression classifier on a deterministic
//!   synthetic dataset; the candidate is its weight vector; metrics: accuracy,
//!   log-loss, inference latency.  Training is gradient descent exposed as a
//!   separate component, so the lab's "train → re-evaluate" loop is real.
//! * [`reasoner`] – a bounded-depth arithmetic reasoner.  Depth trades
//!   correctness for speed, which lets the lab prove that *speed is not
//!   reasoning* (REQ-AIL-4): a faster but shallower candidate must be rejected.
//! * [`bandit`] – ε-greedy agent on a fixed multi-armed bandit; the candidate
//!   is ε and the optimism prior; metric: regret.
//!
//! Real LLM / vision / control models are reached through the Python bridge
//! (PyTorch, Transformers, TRL…) with the same contract; see
//! `services/training-service` and `services/evaluation-service`.

use simorph_core::{Component, ComponentId, ComponentManifest};

pub mod bandit;
pub mod classifier;
pub mod reasoner;

pub fn manifests() -> Vec<ComponentManifest> {
    vec![
        classifier::ClassifierEval::new().manifest().clone(),
        classifier::ClassifierTrain::new().manifest().clone(),
        reasoner::ArithmeticReasoner::new().manifest().clone(),
        bandit::BanditAgent::new().manifest().clone(),
    ]
}

pub fn build(id: ComponentId) -> Option<Box<dyn Component>> {
    let all: Vec<Box<dyn Component>> = vec![
        Box::new(classifier::ClassifierEval::new()),
        Box::new(classifier::ClassifierTrain::new()),
        Box::new(reasoner::ArithmeticReasoner::new()),
        Box::new(bandit::BanditAgent::new()),
    ];
    all.into_iter().find(|c| c.manifest().id == id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn manifests_valid() {
        for m in super::manifests() {
            m.validate_structure().unwrap();
            assert!(super::build(m.id).is_some());
        }
    }
}
