//! Parallel, deterministic execution of experiment plans and reproduction.

use crate::plan::{
    ExperimentPlan, ExperimentRecord, OutputSummary, RunRecord, RunStatus, SuccessCriterion,
};
use chrono::Utc;
use indexmap::IndexMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use simorph_core::{run_component, Component, ExecutionControl, Result, SimorphError, TaskRequest};
use std::time::{Duration, Instant};

fn build_request(plan: &ExperimentPlan, index: usize) -> TaskRequest {
    let mut req = TaskRequest::new(plan.component_id)
        .seed(plan.seed.wrapping_add(index as u64))
        .fidelity(plan.fidelity);
    for (k, v) in &plan.fixed_inputs {
        req.inputs.insert(k.clone(), v.clone());
    }
    let point = &plan.design.points[index];
    for (f, v) in plan.design.factors.iter().zip(point) {
        req.inputs
            .insert(f.name.clone(), serde_json::Value::from(*v));
    }
    req.config = plan.config.clone();
    req
}

fn execute_point(
    plan: &ExperimentPlan,
    component: &dyn Component,
    index: usize,
    parent: &ExecutionControl,
) -> RunRecord {
    let req = build_request(plan, index);
    let ctl = if plan.run_timeout_ms > 0 {
        ExecutionControl {
            cancel: parent.cancel.clone(),
            deadline: Some(simorph_core::Deadline::after(Duration::from_millis(
                plan.run_timeout_ms,
            ))),
        }
    } else {
        ExecutionControl {
            cancel: parent.cancel.clone(),
            deadline: parent.deadline,
        }
    };
    let start = Instant::now();
    let (status, result) = match run_component(component, &req, &ctl) {
        Ok(r) => match non_finite_output(&r) {
            // NaN/inf become JSON null: such a run did not succeed (audit:
            // NaN runs were counted as successes and dropped from summaries).
            Some(name) => (
                RunStatus::Failed {
                    error: format!("non-finite output `{name}`"),
                },
                Some(r),
            ),
            None => (RunStatus::Success, Some(r)),
        },
        Err(SimorphError::Cancelled) => (RunStatus::Cancelled, None),
        Err(SimorphError::Timeout(_)) => (RunStatus::TimedOut, None),
        Err(e) => (
            RunStatus::Failed {
                error: e.to_string(),
            },
            None,
        ),
    };
    RunRecord {
        run_id: req.run_id,
        index,
        inputs: req.inputs,
        seed: req.seed.unwrap_or(0),
        status,
        result,
        wall_time_ms: start.elapsed().as_millis() as u64,
    }
}

/// Name of the first output that is non-finite (JSON null, or a null
/// element inside an array/object), if any.
pub fn non_finite_output(r: &simorph_core::TaskResult) -> Option<String> {
    fn bad(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Null => true,
            serde_json::Value::Number(n) => n.as_f64().map(|x| !x.is_finite()).unwrap_or(true),
            serde_json::Value::Array(a) => a.iter().any(bad),
            serde_json::Value::Object(o) => o.values().any(bad),
            _ => false,
        }
    }
    r.outputs
        .iter()
        .find(|(_, v)| bad(v))
        .map(|(k, _)| k.clone())
}

fn evaluate_criterion(
    c: &SuccessCriterion,
    runs: &[RunRecord],
    summary: Option<&OutputSummary>,
) -> bool {
    // Summary statistics are computed over successful runs only; failed
    // runs (including non-finite outputs) are accounted for by
    // `fraction_ok` criteria.
    let value = match c.statistic.as_str() {
        "mean" => summary.map(|s| s.mean),
        "max" => summary.map(|s| s.max),
        "min" => summary.map(|s| s.min),
        "fraction_ok" => {
            let n = runs.len();
            if n == 0 {
                None
            } else {
                Some(runs.iter().filter(|r| r.is_success()).count() as f64 / n as f64)
            }
        }
        _ => None,
    };
    let Some(v) = value else { return false };
    match c.comparator.as_str() {
        "<" => v < c.threshold,
        "<=" => v <= c.threshold,
        ">" => v > c.threshold,
        ">=" => v >= c.threshold,
        _ => false,
    }
}

/// Execute every design point of `plan` through `component`, in parallel.
/// Results are collected in design order, so the record is deterministic
/// for a deterministic component regardless of the thread count.
pub fn run_experiment(
    plan: &ExperimentPlan,
    component: &dyn Component,
    ctl: &ExecutionControl,
) -> Result<ExperimentRecord> {
    if plan.design.is_empty() {
        return Err(SimorphError::InvalidInput(
            "experiment design has no points".into(),
        ));
    }
    let started_at = Utc::now();
    let runs: Vec<RunRecord> = (0..plan.design.len())
        .into_par_iter()
        .map(|i| execute_point(plan, component, i, ctl))
        .collect();
    let finished_at = Utc::now();
    let mut summaries = IndexMap::new();
    let mut names: Vec<String> = vec![];
    for r in &runs {
        if let Some(res) = &r.result {
            for k in res.outputs.keys() {
                if !names.contains(k) {
                    names.push(k.clone());
                }
            }
        }
    }
    for name in names {
        let vals: Vec<f64> = runs
            .iter()
            .filter(|r| r.is_success())
            .filter_map(|r| r.output_f64(&name))
            .collect();
        if let Some(s) = OutputSummary::of(&vals) {
            summaries.insert(name, s);
        }
    }
    let criteria_results = plan
        .success_criteria
        .iter()
        .map(|c| {
            (
                c.clone(),
                evaluate_criterion(c, &runs, summaries.get(&c.output)),
            )
        })
        .collect();
    let hashes: Vec<String> = runs
        .iter()
        .map(|r| {
            r.result
                .as_ref()
                .map(|x| x.output_hash())
                .unwrap_or_default()
        })
        .collect();
    Ok(ExperimentRecord {
        id: plan.id,
        plan: plan.clone(),
        plan_hash: plan.content_hash(),
        component_hash: component.manifest().content_hash(),
        started_at,
        finished_at,
        runs,
        summaries,
        criteria_results,
        outputs_hash: simorph_core::hashing::hash_json(&hashes)?,
    })
}

/// Outcome of re-running an archived experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReproductionReport {
    pub original: simorph_core::ExperimentId,
    pub component_hash_matches: bool,
    pub n_runs: usize,
    /// Per run: output hashes identical.
    pub hash_equal: Vec<bool>,
    /// Largest |new - old| / max(1, |old|) over all numeric outputs.
    pub max_relative_deviation: f64,
    /// Runs whose status differs between original and reproduction.
    pub status_mismatches: Vec<usize>,
    /// Runs whose output keys or array shapes differ.
    #[serde(default)]
    pub structure_mismatches: Vec<usize>,
}

impl ReproductionReport {
    pub fn bitwise_identical(&self) -> bool {
        self.hash_equal.iter().all(|&b| b)
            && self.status_mismatches.is_empty()
            && self.structure_mismatches.is_empty()
    }
    pub fn within(&self, rtol: f64) -> bool {
        self.status_mismatches.is_empty()
            && self.structure_mismatches.is_empty()
            && self.max_relative_deviation <= rtol
    }
}

/// Structural signature: keys and array lengths, recursively.
fn shape(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Array(a) => format!(
            "[{}:{}]",
            a.len(),
            a.iter().map(shape).collect::<Vec<_>>().join(",")
        ),
        serde_json::Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, x)| format!("{k}={}", shape(x)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        serde_json::Value::Number(_) => "n".into(),
        serde_json::Value::String(_) => "s".into(),
        serde_json::Value::Bool(_) => "b".into(),
        serde_json::Value::Null => "0".into(),
    }
}

fn numeric_leaves(v: &serde_json::Value, out: &mut Vec<f64>) {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                out.push(f)
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| numeric_leaves(x, out)),
        serde_json::Value::Object(o) => o.values().for_each(|x| numeric_leaves(x, out)),
        _ => {}
    }
}

/// Re-run an archived experiment with the same seeds and compare.
pub fn reproduce(
    record: &ExperimentRecord,
    component: &dyn Component,
    ctl: &ExecutionControl,
) -> Result<ReproductionReport> {
    let fresh = run_experiment(&record.plan, component, ctl)?;
    let mut hash_equal = vec![];
    let mut max_dev: f64 = 0.0;
    let mut status_mismatches = vec![];
    let mut structure_mismatches = vec![];
    if record.runs.len() != fresh.runs.len() {
        structure_mismatches.push(usize::MAX);
    }
    for (old, new) in record.runs.iter().zip(fresh.runs.iter()) {
        let same_status =
            std::mem::discriminant(&old.status) == std::mem::discriminant(&new.status);
        if !same_status {
            status_mismatches.push(old.index);
        }
        match (&old.result, &new.result) {
            (Some(a), Some(b)) => {
                hash_equal.push(a.output_hash() == b.output_hash());
                let sa: Vec<(String, String)> = a
                    .outputs
                    .iter()
                    .map(|(k, v)| (k.clone(), shape(v)))
                    .collect();
                let sb: Vec<(String, String)> = b
                    .outputs
                    .iter()
                    .map(|(k, v)| (k.clone(), shape(v)))
                    .collect();
                if sa != sb {
                    structure_mismatches.push(old.index);
                }
                let (mut la, mut lb) = (vec![], vec![]);
                for (k, va) in &a.outputs {
                    if let Some(vb) = b.outputs.get(k) {
                        numeric_leaves(va, &mut la);
                        numeric_leaves(vb, &mut lb);
                    }
                }
                for (x, y) in la.iter().zip(lb.iter()) {
                    let d = (x - y).abs() / x.abs().max(1.0);
                    if d > max_dev {
                        max_dev = d;
                    }
                }
            }
            (None, None) => hash_equal.push(true),
            _ => hash_equal.push(false),
        }
    }
    Ok(ReproductionReport {
        original: record.id,
        component_hash_matches: fresh.component_hash == record.component_hash,
        n_runs: record.runs.len(),
        hash_equal,
        max_relative_deviation: max_dev,
        status_mismatches,
        structure_mismatches,
    })
}
