//! Sealed evaluation suite and sampling helpers.

use crate::targets::TargetSystem;
use serde_json::json;
use simorph_core::*;
use simorph_experiment::{run_experiment, Design, ExperimentPlan, ExperimentRecord, Factor};
use simorph_validation::{checks, ProtectedReferences, ReferenceOrigin};

/// A box in *transformed* feature space.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    pub index: Vec<usize>,
    pub low: Vec<f64>,
    pub high: Vec<f64>,
}

/// Latin-hypercube points in transformed space (inside `cell` or the whole
/// domain), returned in physical units.
pub fn lhs_points(
    target: &TargetSystem,
    n: usize,
    seed: u64,
    cell: Option<&Cell>,
) -> Result<Vec<Vec<f64>>> {
    if n == 0 {
        return Ok(vec![]);
    }
    let factors: Vec<Factor> = target
        .inputs
        .iter()
        .enumerate()
        .map(|(j, d)| {
            let (lo, hi) = match cell {
                Some(c) => (c.low[j], c.high[j]),
                None => (d.t_low(), d.t_high()),
            };
            Factor::new(&d.name, "transformed", lo, hi)
        })
        .collect();
    let design = Design::latin_hypercube(factors, n, seed)?;
    Ok(design
        .points
        .iter()
        .map(|p| {
            p.iter()
                .zip(&target.inputs)
                .map(|(v, d)| d.transform.inverse(*v))
                .collect()
        })
        .collect())
}

/// One validated simulator sample.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sample {
    pub inputs: Vec<f64>,
    pub output: f64,
    pub run_id: RunId,
}

/// Result of running + independently validating a batch of design points.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidatedBatch {
    pub samples: Vec<Sample>,
    pub rejected: Vec<(Vec<f64>, String)>,
    pub reference_checks_passed: bool,
    pub experiment_id: ExperimentId,
    pub outputs_hash: String,
}

/// Run `points` through the simulator with `simorph_experiment::run_experiment`
/// and validate every sample independently: manifest invariants, the
/// component's reference cases (once per batch), and the target's physical bounds.
pub fn run_and_validate(
    target: &TargetSystem,
    points: &[Vec<f64>],
    seed: u64,
    name: &str,
    ctl: &ExecutionControl,
) -> Result<(ValidatedBatch, ExperimentRecord)> {
    let factors: Vec<Factor> = target
        .inputs
        .iter()
        .map(|d| Factor::new(&d.name, &d.unit, d.low, d.high))
        .collect();
    let design = Design::explicit(factors, points.to_vec())?;
    let mut plan = ExperimentPlan::new(name, target.component.manifest().id, design, seed);
    for (k, v) in &target.fixed {
        plan = plan.fixed(k, json!(v));
    }
    let record = run_experiment(&plan, target.component.as_ref(), ctl)?;
    let refs = checks::check_reference_cases(
        target.component.as_ref(),
        &target
            .component
            .manifest()
            .validation_protocol
            .reference_cases,
        ctl,
    );
    let reference_ok = refs.iter().all(|c| c.passed);
    let invariants = &target.component.manifest().validation_protocol.invariants;
    let mut samples = vec![];
    let mut rejected = vec![];
    for (run, x) in record.runs.iter().zip(points) {
        let Some(res) = &run.result else {
            rejected.push((x.clone(), format!("run failed: {:?}", run.status)));
            continue;
        };
        if !reference_ok {
            rejected.push((
                x.clone(),
                "component reference cases failed for this batch".into(),
            ));
            continue;
        }
        let inv = checks::check_invariants(invariants, res);
        if !inv.passed {
            rejected.push((x.clone(), format!("invariant: {}", inv.details)));
            continue;
        }
        let y = match res.get_f64(&target.output) {
            Ok(v) => v,
            Err(e) => {
                rejected.push((x.clone(), e.to_string()));
                continue;
            }
        };
        if let Err(why) = (target.bounds)(&target.input_map(x), y) {
            rejected.push((x.clone(), format!("physical bound: {why}")));
            continue;
        }
        samples.push(Sample {
            inputs: x.clone(),
            output: y,
            run_id: run.run_id,
        });
    }
    Ok((
        ValidatedBatch {
            samples,
            rejected,
            reference_checks_passed: reference_ok,
            experiment_id: record.id,
            outputs_hash: record.outputs_hash.clone(),
        },
        record,
    ))
}

/// Sealed held-out suite.  The underlying `ProtectedReferences` refuses any
/// mutation once sealed and is verified before every read.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SealedEvaluationSuite {
    refs: ProtectedReferences,
    input_names: Vec<String>,
    output: String,
}

impl SealedEvaluationSuite {
    /// Build from validated simulator samples (LHS over the whole domain,
    /// seed distinct from any training seed) and seal.
    pub fn build(
        target: &TargetSystem,
        n: usize,
        seed: u64,
        ctl: &ExecutionControl,
    ) -> Result<Self> {
        let pts = lhs_points(target, n, seed, None)?;
        let (batch, _) = run_and_validate(
            target,
            &pts,
            seed,
            &format!("suite scellée {}", target.name),
            ctl,
        )?;
        if !batch.rejected.is_empty() {
            return Err(SimorphError::Validation(format!(
                "{} suite samples failed validation: {:?}",
                batch.rejected.len(),
                batch.rejected.first()
            )));
        }
        let mut refs = ProtectedReferences::new(&format!("suite-{}", target.name));
        for (i, s) in batch.samples.iter().enumerate() {
            let inputs: serde_json::Map<String, serde_json::Value> = target
                .inputs
                .iter()
                .zip(&s.inputs)
                .map(|(d, v)| (d.name.clone(), json!(v)))
                .collect();
            refs.add(
                ReferenceCase {
                    name: format!("p{i:04}"),
                    inputs: serde_json::Value::Object(inputs),
                    expected_outputs: json!({ target.output.clone(): s.output }),
                    rtol: 0.0,
                    atol: 0.0,
                    // Content-addressed: the seal must depend on the science only,
                    // not on run identifiers (those are kept in provenance).
                    source: format!(
                        "validated simulation by {}@{}",
                        target.component.manifest().name,
                        target.component.manifest().version
                    ),
                },
                ReferenceOrigin::Numerical,
            )?;
        }
        refs.seal();
        Ok(Self {
            refs,
            input_names: target.inputs.iter().map(|d| d.name.clone()).collect(),
            output: target.output.clone(),
        })
    }

    pub fn hash(&self) -> String {
        self.refs.seal_hash().unwrap_or_default().to_string()
    }
    pub fn len(&self) -> usize {
        self.refs.cases().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Verified read access: `(inputs, expected output)` pairs.
    pub fn points(&self) -> Result<Vec<(Vec<f64>, f64)>> {
        self.refs.verify()?;
        self.refs
            .cases()
            .iter()
            .map(|c| {
                let x = self
                    .input_names
                    .iter()
                    .map(|n| {
                        c.case.inputs[n].as_f64().ok_or_else(|| {
                            SimorphError::Validation(format!("suite input `{n}` missing"))
                        })
                    })
                    .collect::<Result<Vec<f64>>>()?;
                let y = c.case.expected_outputs[&self.output]
                    .as_f64()
                    .ok_or_else(|| SimorphError::Validation("suite output missing".into()))?;
                Ok((x, y))
            })
            .collect()
    }

    /// Any attempt to add a case is refused by the sealed reference set.
    pub fn try_add(&mut self, case: ReferenceCase) -> Result<()> {
        self.refs.add(case, ReferenceOrigin::Numerical)
    }
    /// Any attempt to remove a case is refused by the sealed reference set.
    pub fn try_remove(&mut self, name: &str) -> Result<()> {
        self.refs.remove(name)
    }
}
