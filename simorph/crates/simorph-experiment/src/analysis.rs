//! Counter-example search and failure-cause attribution.

use crate::design::{Design, Factor};
use crate::plan::{ExperimentRecord, RunRecord};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{run_component, Component, ExecutionControl, Result, TaskRequest, TaskResult};

/// An input point that violates a predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterExample {
    pub inputs: IndexMap<String, serde_json::Value>,
    pub result: TaskResult,
    pub evaluations: usize,
}

/// Outcome of a counter-example search with its error accounting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterExampleSearch {
    pub counter_example: Option<CounterExample>,
    pub evaluations: usize,
    /// Evaluations whose run failed (skipped for the predicate).
    pub errors: usize,
    /// First error message, if any.
    pub first_error: Option<String>,
}

/// Search (Latin hypercube batches of `batch`) for an input where
/// `predicate(result)` is false, i.e. a violation.  Returns the first
/// violation found or `None` after `max_evaluations`.  Returns an error when
/// every evaluation failed (no evidence either way); see
/// [`search_counter_example`] for the error count.
pub fn find_counter_example<F>(
    component: &dyn Component,
    factors: Vec<Factor>,
    fixed_inputs: &IndexMap<String, serde_json::Value>,
    predicate: F,
    max_evaluations: usize,
    batch: usize,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<Option<CounterExample>>
where
    F: Fn(&TaskResult) -> bool,
{
    let s = search_counter_example(
        component,
        factors,
        fixed_inputs,
        predicate,
        max_evaluations,
        batch,
        seed,
        ctl,
    )?;
    if s.counter_example.is_none() && s.evaluations > 0 && s.errors == s.evaluations {
        return Err(simorph_core::SimorphError::Validation(format!(
            "counter-example search inconclusive: all {} evaluations failed (first error: {})",
            s.evaluations,
            s.first_error.unwrap_or_default()
        )));
    }
    Ok(s.counter_example)
}

/// Counter-example search reporting how many evaluations failed.
#[allow(clippy::too_many_arguments)]
pub fn search_counter_example<F>(
    component: &dyn Component,
    factors: Vec<Factor>,
    fixed_inputs: &IndexMap<String, serde_json::Value>,
    predicate: F,
    max_evaluations: usize,
    batch: usize,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<CounterExampleSearch>
where
    F: Fn(&TaskResult) -> bool,
{
    let mut evaluated = 0usize;
    let mut errors = 0usize;
    let mut first_error = None;
    let mut round = 0u64;
    let batch = batch.max(1);
    while evaluated < max_evaluations {
        ctl.check()?;
        let design = Design::latin_hypercube(factors.clone(), batch, seed.wrapping_add(round))?;
        for point in &design.points {
            if evaluated >= max_evaluations {
                break;
            }
            let mut req =
                TaskRequest::new(component.manifest().id).seed(seed.wrapping_add(evaluated as u64));
            for (k, v) in fixed_inputs {
                req.inputs.insert(k.clone(), v.clone());
            }
            for (f, v) in factors.iter().zip(point) {
                req.inputs
                    .insert(f.name.clone(), serde_json::Value::from(*v));
            }
            evaluated += 1;
            match run_component(component, &req, ctl) {
                Ok(res) => {
                    if !predicate(&res) {
                        return Ok(CounterExampleSearch {
                            counter_example: Some(CounterExample {
                                inputs: req.inputs,
                                result: res,
                                evaluations: evaluated,
                            }),
                            evaluations: evaluated,
                            errors,
                            first_error,
                        });
                    }
                }
                Err(simorph_core::SimorphError::Cancelled) => {
                    return Err(simorph_core::SimorphError::Cancelled)
                }
                Err(e) => {
                    errors += 1;
                    if first_error.is_none() {
                        first_error = Some(e.to_string());
                    }
                }
            }
        }
        round += 1;
    }
    Ok(CounterExampleSearch {
        counter_example: None,
        evaluations: evaluated,
        errors,
        first_error,
    })
}

/// Association between a factor and the failure/violation flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CauseScore {
    pub factor: String,
    /// (mean over bad runs − mean over good runs) / pooled std.
    pub standardised_mean_difference: f64,
    /// Spearman rank correlation between the factor and the bad flag.
    pub rank_correlation: f64,
    /// |smd| + |rho|; ranking key.
    pub score: f64,
    pub n_bad: usize,
    pub n_good: usize,
}

fn ranks(values: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut r = vec![0.0; values.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && values[idx[j + 1]] == values[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            r[idx[k]] = avg;
        }
        i = j + 1;
    }
    r
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let cov: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let vx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let vy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    if vx == 0.0 || vy == 0.0 {
        0.0
    } else {
        cov / (vx * vy).sqrt()
    }
}

/// Rank factors by their association with "bad" runs.  A run is bad when it
/// failed, or when `violates(run)` returns true for a successful run.
pub fn rank_failure_causes<F>(record: &ExperimentRecord, violates: F) -> Vec<CauseScore>
where
    F: Fn(&RunRecord) -> bool,
{
    let bad: Vec<bool> = record
        .runs
        .iter()
        .map(|r| !r.is_success() || violates(r))
        .collect();
    let flag: Vec<f64> = bad.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect();
    let n_bad = bad.iter().filter(|&&b| b).count();
    let n_good = bad.len() - n_bad;
    let mut out = vec![];
    for (j, f) in record.plan.design.factors.iter().enumerate() {
        let col: Vec<f64> = record
            .runs
            .iter()
            .map(|r| {
                r.inputs
                    .get(&f.name)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(record.plan.design.points[r.index][j])
            })
            .collect();
        let (mut sb, mut sg) = (0.0, 0.0);
        for (v, &b) in col.iter().zip(&bad) {
            if b {
                sb += v
            } else {
                sg += v
            }
        }
        let smd = if n_bad > 0 && n_good > 0 {
            let mb = sb / n_bad as f64;
            let mg = sg / n_good as f64;
            let mut ss = 0.0;
            for (v, &b) in col.iter().zip(&bad) {
                let m = if b { mb } else { mg };
                ss += (v - m).powi(2);
            }
            let pooled = (ss / (bad.len() as f64 - 2.0).max(1.0)).sqrt();
            if pooled > 0.0 {
                (mb - mg) / pooled
            } else {
                0.0
            }
        } else {
            0.0
        };
        let rho = pearson(&ranks(&col), &ranks(&flag));
        out.push(CauseScore {
            factor: f.name.clone(),
            standardised_mean_difference: smd,
            rank_correlation: rho,
            score: smd.abs() + rho.abs(),
            n_bad,
            n_good,
        });
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}
