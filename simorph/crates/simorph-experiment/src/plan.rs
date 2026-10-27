//! Experiment plans and records.

use crate::design::Design;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{ComponentId, ExperimentId, HypothesisId, ProblemId, RunId, TaskResult};

/// Success criterion evaluated on an output's summary statistic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuccessCriterion {
    pub output: String,
    /// `"mean" | "max" | "min" | "fraction_ok"`.
    pub statistic: String,
    /// Expression: `"<" | "<=" | ">" | ">="`.
    pub comparator: String,
    pub threshold: f64,
}

/// Plan of an experiment: which component, which design, which fixed
/// inputs, and what counts as success.  Independent of its execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentPlan {
    pub id: ExperimentId,
    pub name: String,
    #[serde(default)]
    pub problem_id: Option<ProblemId>,
    #[serde(default)]
    pub hypothesis_ids: Vec<HypothesisId>,
    pub component_id: ComponentId,
    pub design: Design,
    /// Inputs identical for every point.
    #[serde(default)]
    pub fixed_inputs: IndexMap<String, serde_json::Value>,
    /// Base seed; run `i` uses `seed + i`.
    pub seed: u64,
    #[serde(default = "one")]
    pub fidelity: f64,
    #[serde(default)]
    pub success_criteria: Vec<SuccessCriterion>,
    /// Per-run timeout in milliseconds (0 = none).
    #[serde(default)]
    pub run_timeout_ms: u64,
    /// Extra configuration passed verbatim to the component.
    #[serde(default)]
    pub config: serde_json::Value,
}

fn one() -> f64 {
    1.0
}

impl ExperimentPlan {
    pub fn new(name: &str, component_id: ComponentId, design: Design, seed: u64) -> Self {
        Self {
            id: ExperimentId::new(),
            name: name.into(),
            problem_id: None,
            hypothesis_ids: vec![],
            component_id,
            design,
            fixed_inputs: IndexMap::new(),
            seed,
            fidelity: 1.0,
            success_criteria: vec![],
            run_timeout_ms: 0,
            config: serde_json::Value::Null,
        }
    }
    pub fn fixed(mut self, name: &str, v: impl Into<serde_json::Value>) -> Self {
        self.fixed_inputs.insert(name.into(), v.into());
        self
    }
    pub fn with_criterion(mut self, c: SuccessCriterion) -> Self {
        self.success_criteria.push(c);
        self
    }
    /// Content hash of the plan (same plan ⇒ same hash, independent of id).
    pub fn content_hash(&self) -> String {
        #[derive(Serialize)]
        struct Key<'a> {
            component_id: &'a ComponentId,
            design: &'a Design,
            fixed_inputs: &'a IndexMap<String, serde_json::Value>,
            seed: u64,
            fidelity: f64,
            config: &'a serde_json::Value,
        }
        simorph_core::hashing::hash_json(&Key {
            component_id: &self.component_id,
            design: &self.design,
            fixed_inputs: &self.fixed_inputs,
            seed: self.seed,
            fidelity: self.fidelity,
            config: &self.config,
        })
        .expect("serialisable")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunStatus {
    Success,
    Failed { error: String },
    Cancelled,
    TimedOut,
}

/// Record of a single run (one design point).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RunRecord {
    pub run_id: RunId,
    pub index: usize,
    pub inputs: IndexMap<String, serde_json::Value>,
    pub seed: u64,
    pub status: RunStatus,
    #[serde(default)]
    pub result: Option<TaskResult>,
    pub wall_time_ms: u64,
}

impl RunRecord {
    pub fn is_success(&self) -> bool {
        matches!(self.status, RunStatus::Success)
    }
    pub fn output_f64(&self, name: &str) -> Option<f64> {
        self.result
            .as_ref()
            .and_then(|r| r.outputs.get(name))
            .and_then(|v| v.as_f64())
    }
}

/// Summary statistics of one numeric output over the successful runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputSummary {
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
}

impl OutputSummary {
    pub fn of(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let var = if values.len() > 1 {
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)
        } else {
            0.0
        };
        Some(Self {
            count: values.len(),
            mean,
            std_dev: var.sqrt(),
            min: values.iter().cloned().fold(f64::INFINITY, f64::min),
            max: values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        })
    }
}

/// Complete record of an executed experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentRecord {
    pub id: ExperimentId,
    pub plan: ExperimentPlan,
    pub plan_hash: String,
    /// Hash of the component manifest used.
    pub component_hash: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub runs: Vec<RunRecord>,
    pub summaries: IndexMap<String, OutputSummary>,
    /// Evaluation of each success criterion.
    pub criteria_results: Vec<(SuccessCriterion, bool)>,
    /// Hash over all run output hashes, in order (`""` for failed runs).
    pub outputs_hash: String,
}

impl ExperimentRecord {
    pub fn n_success(&self) -> usize {
        self.runs.iter().filter(|r| r.is_success()).count()
    }
    pub fn n_failed(&self) -> usize {
        self.runs.len() - self.n_success()
    }
    /// True only when success criteria were defined and all of them hold
    /// (audit: an experiment without criteria used to count as "met").
    pub fn all_criteria_met(&self) -> bool {
        self.criteria_defined() && self.criteria_results.iter().all(|(_, ok)| *ok)
    }
    pub fn criteria_defined(&self) -> bool {
        !self.criteria_results.is_empty()
    }
    pub fn output_column(&self, name: &str) -> Vec<Option<f64>> {
        self.runs.iter().map(|r| r.output_f64(name)).collect()
    }
}
