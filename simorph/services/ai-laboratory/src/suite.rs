//! Evaluation suites and candidates.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;

/// What a metric measures.  Only `Quality`-like kinds may be the primary
/// acceptance metric; `Speed`/`Cost` can never justify acceptance on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// Correctness / reasoning quality (accuracy, exact match, loss…).
    Quality,
    /// Robustness under perturbation.
    Robustness,
    /// Held-out generalisation.
    Generalisation,
    /// Latency / throughput.
    Speed,
    /// Compute, memory or monetary cost.
    Cost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MetricSpec {
    pub name: String,
    /// Output of the task component read for this metric.
    pub output: String,
    pub kind: MetricKind,
    pub higher_is_better: bool,
    /// Protected metrics may never regress beyond `regression_tolerance`.
    pub protected: bool,
    #[serde(default)]
    pub regression_tolerance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvalTask {
    pub name: String,
    pub component: ComponentId,
    /// Fixed inputs of the task (candidate parameters override these).
    pub inputs: IndexMap<String, serde_json::Value>,
    /// Seeds used for repeated evaluation.
    pub seeds: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvaluationSuite {
    pub name: String,
    pub version: String,
    pub tasks: Vec<EvalTask>,
    pub metrics: Vec<MetricSpec>,
}

/// A sealed suite: immutable, content-addressed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SealedSuite {
    suite: EvaluationSuite,
    hash: String,
}

impl SealedSuite {
    pub fn seal(suite: EvaluationSuite) -> Result<Self> {
        if suite.tasks.is_empty() || suite.metrics.is_empty() {
            return Err(SimorphError::InvalidInput(
                "suite needs at least one task and one metric".into(),
            ));
        }
        for t in &suite.tasks {
            if t.seeds.is_empty() {
                return Err(SimorphError::InvalidInput(format!(
                    "task `{}` has no seeds",
                    t.name
                )));
            }
        }
        let hash = simorph_core::hashing::hash_json(&suite)?;
        Ok(Self { suite, hash })
    }
    pub fn suite(&self) -> &EvaluationSuite {
        &self.suite
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
    /// Detect tampering of the inner suite (e.g. after deserialisation).
    pub fn verify(&self) -> Result<()> {
        let h = simorph_core::hashing::hash_json(&self.suite)?;
        if h != self.hash {
            return Err(SimorphError::Protected(format!(
                "evaluation suite `{}` was modified after sealing",
                self.suite.name
            )));
        }
        Ok(())
    }
}

/// An AI system candidate: a set of parameters applied to the evaluation
/// tasks (weights, depth, prompt, config…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Candidate {
    pub id: CandidateId,
    pub name: String,
    pub parameters: IndexMap<String, serde_json::Value>,
    pub parent: Option<CandidateId>,
    /// The hypothesis that motivated this candidate.
    pub hypothesis: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Candidate {
    pub fn new(
        name: &str,
        parameters: IndexMap<String, serde_json::Value>,
        hypothesis: &str,
    ) -> Self {
        Self {
            id: CandidateId::new(),
            name: name.into(),
            parameters,
            parent: None,
            hypothesis: hypothesis.into(),
            created_at: chrono::Utc::now(),
        }
    }
    pub fn child(
        &self,
        name: &str,
        parameters: IndexMap<String, serde_json::Value>,
        hypothesis: &str,
    ) -> Self {
        Self {
            parent: Some(self.id),
            ..Self::new(name, parameters, hypothesis)
        }
    }
    pub fn content_hash(&self) -> String {
        simorph_core::hashing::hash_json(&self.parameters).expect("serialisable")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MetricValue {
    pub mean: f64,
    pub std: f64,
    pub n: usize,
    pub min: f64,
    pub max: f64,
}

impl MetricValue {
    pub fn of(values: &[f64]) -> Self {
        let n = values.len();
        let mean = values.iter().sum::<f64>() / n.max(1) as f64;
        let var = if n > 1 {
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64
        } else {
            0.0
        };
        Self {
            mean,
            std: var.sqrt(),
            n,
            min: values.iter().cloned().fold(f64::INFINITY, f64::min),
            max: values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

/// Result of evaluating a candidate on a sealed suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evaluation {
    pub candidate: CandidateId,
    pub suite_hash: String,
    pub metrics: IndexMap<String, MetricValue>,
    /// Per task, per metric raw values (one per seed).
    pub raw: IndexMap<String, IndexMap<String, Vec<f64>>>,
    pub run_ids: Vec<RunId>,
    pub validation: Option<ValidationId>,
    /// Evidence level granted by the validator for the evaluation runs.
    pub evidence: EvidenceLevel,
    pub failures: Vec<String>,
    pub wall_time_s: f64,
}

/// A limit identified from an evaluation (§5.1 "identification des limites").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Limit {
    pub metric: String,
    pub kind: MetricKind,
    pub value: f64,
    pub worst_task: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_suite_detects_tampering() {
        let s = EvaluationSuite {
            name: "s".into(),
            version: "1".into(),
            tasks: vec![EvalTask {
                name: "t".into(),
                component: ComponentId::new(),
                inputs: IndexMap::new(),
                seeds: vec![1],
            }],
            metrics: vec![MetricSpec {
                name: "acc".into(),
                output: "accuracy".into(),
                kind: MetricKind::Quality,
                higher_is_better: true,
                protected: true,
                regression_tolerance: 0.0,
            }],
        };
        let sealed = SealedSuite::seal(s).unwrap();
        sealed.verify().unwrap();
        let mut json: serde_json::Value = serde_json::to_value(&sealed).unwrap();
        json["suite"]["tasks"][0]["seeds"] = serde_json::json!([2]);
        let tampered: SealedSuite = serde_json::from_value(json).unwrap();
        assert!(matches!(tampered.verify(), Err(SimorphError::Protected(_))));
    }
}
