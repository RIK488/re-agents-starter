//! Standard sealed suites built from `plugins/ai`.

use indexmap::IndexMap;
use serde_json::json;
use simorph_core::Component;
use simorph_plugin_ai as ai;
use simorph_svc_ai_laboratory::{EvalTask, EvaluationSuite, MetricKind, MetricSpec, SealedSuite};

fn inputs(pairs: &[(&str, serde_json::Value)]) -> IndexMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn metric(name: &str, output: &str, kind: MetricKind, higher: bool, protected: bool) -> MetricSpec {
    MetricSpec {
        name: name.into(),
        output: output.into(),
        kind,
        higher_is_better: higher,
        protected,
        regression_tolerance: 0.0,
    }
}

/// Classifier on held-out seeds (quality: accuracy, log-loss; speed: latency).
pub fn classifier_suite() -> SealedSuite {
    let id = ai::classifier::ClassifierEval::new().manifest().id;
    SealedSuite::seal(EvaluationSuite {
        name: "standard.classifier.heldout".into(),
        version: "1".into(),
        tasks: vec![
            EvalTask {
                name: "heldout-a".into(),
                component: id,
                inputs: inputs(&[("dataset_seed", json!(901)), ("n_samples", json!(800))]),
                seeds: vec![1],
            },
            EvalTask {
                name: "heldout-b".into(),
                component: id,
                inputs: inputs(&[("dataset_seed", json!(902)), ("n_samples", json!(800))]),
                seeds: vec![1],
            },
            EvalTask {
                name: "heldout-c".into(),
                component: id,
                inputs: inputs(&[("dataset_seed", json!(903)), ("n_samples", json!(800))]),
                seeds: vec![1],
            },
        ],
        metrics: vec![
            metric("accuracy", "accuracy", MetricKind::Quality, true, true),
            metric("log_loss", "log_loss", MetricKind::Quality, false, true),
            metric(
                "latency_per_sample",
                "latency_per_sample",
                MetricKind::Speed,
                false,
                false,
            ),
        ],
    })
    .expect("valid suite")
}

/// Arithmetic reasoner at several task depths (quality: accuracy per depth;
/// cost: mean steps).
pub fn reasoner_suite() -> SealedSuite {
    let id = ai::reasoner::ArithmeticReasoner::new().manifest().id;
    let tasks = [3usize, 5, 7]
        .iter()
        .map(|d| EvalTask {
            name: format!("depth-{d}"),
            component: id,
            inputs: inputs(&[
                ("task_depth", json!(d)),
                ("n_tasks", json!(150)),
                ("task_seed", json!(17)),
            ]),
            seeds: vec![1],
        })
        .collect();
    SealedSuite::seal(EvaluationSuite {
        name: "standard.reasoner.arithmetic".into(),
        version: "1".into(),
        tasks,
        metrics: vec![
            metric("accuracy", "accuracy", MetricKind::Quality, true, true),
            metric("mean_steps", "mean_steps", MetricKind::Cost, false, false),
            metric(
                "latency_per_task",
                "latency_per_task",
                MetricKind::Speed,
                false,
                false,
            ),
        ],
    })
    .expect("valid suite")
}

/// Bandit agent regret on two arm configurations and several seeds.
pub fn bandit_suite() -> SealedSuite {
    let id = ai::bandit::BanditAgent::new().manifest().id;
    SealedSuite::seal(EvaluationSuite {
        name: "standard.agent.bandit".into(),
        version: "1".into(),
        tasks: vec![
            EvalTask {
                name: "easy".into(),
                component: id,
                inputs: inputs(&[
                    ("arm_probabilities", json!([0.2, 0.5, 0.8])),
                    ("horizon", json!(300)),
                    ("episodes", json!(10)),
                ]),
                seeds: vec![1, 2, 3],
            },
            EvalTask {
                name: "hard".into(),
                component: id,
                inputs: inputs(&[
                    ("arm_probabilities", json!([0.45, 0.5, 0.55])),
                    ("horizon", json!(300)),
                    ("episodes", json!(10)),
                ]),
                seeds: vec![1, 2, 3],
            },
        ],
        metrics: vec![
            metric(
                "mean_regret",
                "mean_regret",
                MetricKind::Quality,
                false,
                true,
            ),
            metric(
                "mean_reward",
                "mean_reward",
                MetricKind::Robustness,
                true,
                false,
            ),
        ],
    })
    .expect("valid suite")
}

/// All standard suites.
pub fn standard_suites() -> Vec<SealedSuite> {
    vec![classifier_suite(), reasoner_suite(), bandit_suite()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_suites_are_sealed_and_distinct() {
        let s = super::standard_suites();
        assert_eq!(s.len(), 3);
        for x in &s {
            x.verify().unwrap();
        }
        assert_ne!(s[0].hash(), s[1].hash());
    }
}
