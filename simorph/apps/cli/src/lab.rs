//! AI laboratory actions: standard sealed suites and the improvement loop.

use crate::workspace::Workspace;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use simorph_core::*;
use simorph_plugin_ai as ai;
use simorph_svc_ai_laboratory::*;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteInfo {
    pub name: String,
    pub hash: String,
    pub tasks: usize,
    pub metrics: Vec<String>,
    pub primary_metric: String,
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

/// Standard suites.  They are sealed at construction; their hash is stable
/// for a given SIMORPH version.
pub fn standard_suites() -> Result<Vec<(SealedSuite, AcceptancePolicy)>> {
    let classifier = SealedSuite::seal(EvaluationSuite {
        name: "classifier".into(),
        version: "1".into(),
        tasks: vec![
            EvalTask {
                name: "heldout-a".into(),
                component: ai::classifier::ClassifierEval::new().manifest().id,
                inputs: [
                    ("dataset_seed".to_string(), json!(99)),
                    ("n_samples".to_string(), json!(1000)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1, 2],
            },
            EvalTask {
                name: "heldout-b".into(),
                component: ai::classifier::ClassifierEval::new().manifest().id,
                inputs: [
                    ("dataset_seed".to_string(), json!(1234)),
                    ("n_samples".to_string(), json!(1000)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1, 2],
            },
        ],
        metrics: vec![
            metric("accuracy", "accuracy", MetricKind::Quality, true, true),
            metric("log_loss", "log_loss", MetricKind::Quality, false, true),
            metric(
                "latency",
                "latency_per_sample",
                MetricKind::Speed,
                false,
                false,
            ),
        ],
    })?;
    let reasoner = SealedSuite::seal(EvaluationSuite {
        name: "reasoner".into(),
        version: "1".into(),
        tasks: (3..=6)
            .map(|d| EvalTask {
                name: format!("depth-{d}"),
                component: ai::reasoner::ArithmeticReasoner::new().manifest().id,
                inputs: [
                    ("task_depth".to_string(), json!(d)),
                    ("n_tasks".to_string(), json!(150)),
                    ("task_seed".to_string(), json!(10 + d)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1],
            })
            .collect(),
        metrics: vec![
            metric("accuracy", "accuracy", MetricKind::Quality, true, true),
            metric("steps", "mean_steps", MetricKind::Cost, false, false),
            metric(
                "latency",
                "latency_per_task",
                MetricKind::Speed,
                false,
                false,
            ),
        ],
    })?;
    let bandit = SealedSuite::seal(EvaluationSuite {
        name: "bandit".into(),
        version: "1".into(),
        tasks: vec![EvalTask {
            name: "3-arms".into(),
            component: ai::bandit::BanditAgent::new().manifest().id,
            inputs: [
                ("arm_probabilities".to_string(), json!([0.2, 0.5, 0.8])),
                ("horizon".to_string(), json!(400)),
                ("episodes".to_string(), json!(20)),
            ]
            .into_iter()
            .collect(),
            seeds: vec![1, 2, 3],
        }],
        metrics: vec![
            metric("regret", "mean_regret", MetricKind::Quality, false, true),
            metric("reward", "mean_reward", MetricKind::Quality, true, false),
        ],
    })?;
    Ok(vec![
        (
            classifier,
            AcceptancePolicy {
                primary_metric: "accuracy".into(),
                min_relative_improvement: 0.02,
                min_z_score: 1.0,
                min_evidence: EvidenceLevel::Computed,
            },
        ),
        (
            reasoner,
            AcceptancePolicy {
                primary_metric: "accuracy".into(),
                min_relative_improvement: 0.01,
                min_z_score: 0.0,
                min_evidence: EvidenceLevel::Computed,
            },
        ),
        (
            bandit,
            AcceptancePolicy {
                primary_metric: "regret".into(),
                min_relative_improvement: 0.05,
                min_z_score: 1.0,
                min_evidence: EvidenceLevel::Simulated,
            },
        ),
    ])
}

pub fn suites() -> Result<Vec<SuiteInfo>> {
    Ok(standard_suites()?
        .into_iter()
        .map(|(s, p)| SuiteInfo {
            name: s.suite().name.clone(),
            hash: s.hash().to_string(),
            tasks: s.suite().tasks.len(),
            metrics: s.suite().metrics.iter().map(|m| m.name.clone()).collect(),
            primary_metric: p.primary_metric,
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabRunOutput {
    pub suite: String,
    pub suite_hash: String,
    pub iterations: Vec<Iteration>,
    pub final_candidate: Candidate,
    pub final_metrics: IndexMap<String, MetricValue>,
    pub report_markdown: String,
}

fn resolver() -> Arc<dyn ComponentResolver> {
    Arc::new(FnResolver(|id: ComponentId| {
        ai::build(id).map(|b| Arc::from(b) as Arc<dyn Component>)
    }))
}

/// Run the improvement loop for a standard suite.
pub fn run_lab(
    ws: Option<&Workspace>,
    suite_name: &str,
    iterations: usize,
    ctl: &ExecutionControl,
) -> Result<LabRunOutput> {
    let (suite, policy) = standard_suites()?
        .into_iter()
        .find(|(s, _)| s.suite().name == suite_name)
        .ok_or_else(|| SimorphError::NotFound(format!("suite `{suite_name}`")))?;
    let ledger = ws
        .map(|w| w.ledger.clone())
        .unwrap_or_else(simorph_provenance::Ledger::in_memory);
    let (baseline, improver): (Candidate, Box<dyn Improver>) = match suite_name {
        "classifier" => (
            Candidate::new(
                "zero-weights",
                [("weights".to_string(), json!([0, 0, 0, 0, 0, 0]))]
                    .into_iter()
                    .collect(),
                "point de départ : poids nuls",
            ),
            Box::new(TrainingImprover {
                trainer: ai::build(ai::classifier::ClassifierTrain::new().manifest().id).unwrap(),
                param: "weights".into(),
                trainer_inputs: [
                    ("dataset_seed".to_string(), json!(1)),
                    ("n_samples".to_string(), json!(1500)),
                    ("learning_rate".to_string(), json!(0.5)),
                    ("epochs".to_string(), json!(150)),
                ]
                .into_iter()
                .collect(),
            }),
        ),
        "reasoner" => (
            Candidate::new(
                "depth-2",
                [("max_depth".to_string(), json!(2))].into_iter().collect(),
                "point de départ : profondeur 2",
            ),
            Box::new(ParameterSweepImprover {
                param: "max_depth".into(),
                deltas: vec![-1.0, 1.0, 2.0],
                min: 0.0,
                max: 12.0,
            }),
        ),
        "bandit" => (
            Candidate::new(
                "greedy",
                [
                    ("epsilon".to_string(), json!(0.0)),
                    ("optimistic_init".to_string(), json!(0.0)),
                ]
                .into_iter()
                .collect(),
                "point de départ : glouton pur",
            ),
            Box::new(ParameterSweepImprover {
                param: "epsilon".into(),
                deltas: vec![0.05, 0.1, 0.3],
                min: 0.0,
                max: 1.0,
            }),
        ),
        _ => {
            return Err(SimorphError::NotFound(format!(
                "no improver for suite `{suite_name}`"
            )))
        }
    };
    let mut lab = Laboratory::new(suite.clone(), policy, resolver(), baseline, ledger)?;
    for _ in 0..iterations.max(1) {
        ctl.check()?;
        lab.iterate(improver.as_ref(), ctl)?;
    }
    let report_markdown = lab.report_markdown();
    if let Some(ws) = ws {
        let dir = ws.root.join("lab").join(format!(
            "{}-{}",
            suite_name,
            chrono::Utc::now().format("%Y%m%dT%H%M%S")
        ));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("report.md"), &report_markdown)?;
        std::fs::write(
            dir.join("history.json"),
            serde_json::to_string_pretty(&lab.history)?,
        )?;
    }
    Ok(LabRunOutput {
        suite: suite_name.into(),
        suite_hash: suite.hash().to_string(),
        iterations: lab.history.clone(),
        final_candidate: lab.current().clone(),
        final_metrics: lab
            .current_evaluation()
            .map(|e| e.metrics.clone())
            .unwrap_or_default(),
        report_markdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_suites_seal_and_reasoner_loop_improves() {
        let s = suites().unwrap();
        assert_eq!(s.len(), 3);
        let out = run_lab(None, "reasoner", 2, &ExecutionControl::default()).unwrap();
        assert!(out.iterations.iter().any(|i| i.decision.accepted()));
        assert!(out.final_metrics["accuracy"].mean > 0.5);
    }
}
