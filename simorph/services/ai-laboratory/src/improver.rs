//! Improvement proposers (hypothesis → candidate).

use crate::suite::{Candidate, Evaluation, Limit};
use indexmap::IndexMap;
use simorph_core::*;

/// Proposes candidate improvements from an evaluation and its limits.
pub trait Improver: Send + Sync {
    fn name(&self) -> &str;
    fn propose(
        &self,
        baseline: &Candidate,
        eval: &Evaluation,
        limits: &[Limit],
        ctl: &ExecutionControl,
    ) -> Result<Vec<Candidate>>;
}

/// Runs a training component to produce new parameters.
/// `param_in`/`param_out` name the candidate parameter that the training
/// component reads and writes (e.g. `weights`).
pub struct TrainingImprover {
    pub trainer: Box<dyn Component>,
    pub param: String,
    pub trainer_inputs: IndexMap<String, serde_json::Value>,
}

impl Improver for TrainingImprover {
    fn name(&self) -> &str {
        "training"
    }
    fn propose(
        &self,
        baseline: &Candidate,
        _eval: &Evaluation,
        limits: &[Limit],
        ctl: &ExecutionControl,
    ) -> Result<Vec<Candidate>> {
        let mut req = TaskRequest::new(self.trainer.manifest().id);
        for (k, v) in &self.trainer_inputs {
            req.inputs.insert(k.clone(), v.clone());
        }
        let current = baseline
            .parameters
            .get(&self.param)
            .cloned()
            .ok_or_else(|| {
                SimorphError::InvalidInput(format!("baseline has no parameter `{}`", self.param))
            })?;
        req.inputs.insert(self.param.clone(), current);
        let res = run_component(self.trainer.as_ref(), &req, ctl)?;
        let new = res
            .outputs
            .get(&self.param)
            .cloned()
            .ok_or_else(|| SimorphError::Component {
                component: self.trainer.manifest().name.clone(),
                message: format!("trainer produced no `{}`", self.param),
            })?;
        let mut params = baseline.parameters.clone();
        params.insert(self.param.clone(), new);
        let why = limits
            .first()
            .map(|l| {
                format!(
                    "entraînement pour améliorer `{}` (pire tâche : {})",
                    l.metric, l.worst_task
                )
            })
            .unwrap_or_else(|| "entraînement".into());
        Ok(vec![baseline.child(
            &format!("{} +train", baseline.name),
            params,
            &why,
        )])
    }
}

/// Proposes neighbouring values of a numeric parameter (ablation / sweep).
pub struct ParameterSweepImprover {
    pub param: String,
    pub deltas: Vec<f64>,
    pub min: f64,
    pub max: f64,
}

impl Improver for ParameterSweepImprover {
    fn name(&self) -> &str {
        "parameter-sweep"
    }
    fn propose(
        &self,
        baseline: &Candidate,
        _eval: &Evaluation,
        _limits: &[Limit],
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Candidate>> {
        let cur = baseline
            .parameters
            .get(&self.param)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                SimorphError::InvalidInput(format!("parameter `{}` is not numeric", self.param))
            })?;
        Ok(self
            .deltas
            .iter()
            .map(|d| (cur + d).clamp(self.min, self.max))
            .filter(|v| *v != cur)
            .map(|v| {
                let mut p = baseline.parameters.clone();
                p.insert(self.param.clone(), serde_json::json!(v));
                baseline.child(
                    &format!("{} [{}={}]", baseline.name, self.param, v),
                    p,
                    &format!(
                        "hypothèse : {} = {} améliore la métrique principale",
                        self.param, v
                    ),
                )
            })
            .collect())
    }
}
