//! Acceptance policy.

use crate::suite::{Evaluation, MetricKind, MetricSpec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcceptancePolicy {
    /// Metric that must improve.  Must be of kind Quality/Robustness/Generalisation.
    pub primary_metric: String,
    /// Minimum relative improvement of the primary metric (e.g. 0.01 = 1 %).
    pub min_relative_improvement: f64,
    /// Minimum z-score of the improvement (Welch); ignored when both
    /// evaluations are deterministic (std = 0), where the relative threshold applies.
    pub min_z_score: f64,
    /// Minimum evidence level the validator must grant to the evaluation runs.
    pub min_evidence: EvidenceLevel,
}

impl AcceptancePolicy {
    pub fn validate(&self, metrics: &[MetricSpec]) -> Result<()> {
        let m = metrics
            .iter()
            .find(|m| m.name == self.primary_metric)
            .ok_or_else(|| {
                SimorphError::InvalidInput(format!(
                    "primary metric `{}` not in suite",
                    self.primary_metric
                ))
            })?;
        if matches!(m.kind, MetricKind::Speed | MetricKind::Cost) {
            return Err(SimorphError::InvalidInput(format!(
                "primary metric `{}` is a {:?} metric: speed/cost can never be the acceptance criterion (REQ-AIL-4)",
                m.name, m.kind
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Accepted { reasons: Vec<String> },
    Rejected { reasons: Vec<String> },
}

impl Decision {
    pub fn accepted(&self) -> bool {
        matches!(self, Decision::Accepted { .. })
    }
}

/// Welch z-score of (candidate − baseline).
pub fn z_score(cand: &crate::suite::MetricValue, base: &crate::suite::MetricValue) -> f64 {
    let se =
        (cand.std.powi(2) / cand.n.max(1) as f64 + base.std.powi(2) / base.n.max(1) as f64).sqrt();
    if se == 0.0 {
        if cand.mean == base.mean {
            0.0
        } else {
            f64::INFINITY * (cand.mean - base.mean).signum()
        }
    } else {
        (cand.mean - base.mean) / se
    }
}

/// Decide whether `cand` replaces `base`.
pub fn decide(
    policy: &AcceptancePolicy,
    metrics: &[MetricSpec],
    base: &Evaluation,
    cand: &Evaluation,
) -> Decision {
    let mut reasons = vec![];
    let mut reject = vec![];
    if base.suite_hash != cand.suite_hash {
        reject.push("les deux évaluations n'utilisent pas la même suite scellée".into());
    }
    if evidence_rank(cand.evidence) < evidence_rank(policy.min_evidence) {
        reject.push(format!(
            "niveau de preuve accordé ({}) insuffisant (requis : {})",
            cand.evidence.label_fr(),
            policy.min_evidence.label_fr()
        ));
    }
    if !cand.failures.is_empty() {
        reject.push(format!(
            "{} échec(s) d'exécution pendant l'évaluation",
            cand.failures.len()
        ));
    }
    let spec = metrics.iter().find(|m| m.name == policy.primary_metric);
    match (
        spec,
        base.metrics.get(&policy.primary_metric),
        cand.metrics.get(&policy.primary_metric),
    ) {
        (Some(spec), Some(b), Some(c)) => {
            let sign = if spec.higher_is_better { 1.0 } else { -1.0 };
            let rel = sign * (c.mean - b.mean) / b.mean.abs().max(1e-12);
            let z = sign * z_score(c, b);
            let deterministic = c.std == 0.0 && b.std == 0.0;
            if rel >= policy.min_relative_improvement && (deterministic || z >= policy.min_z_score)
            {
                reasons.push(format!(
                    "métrique principale `{}` : {:.4} → {:.4} (+{:.2} %, z = {:.2})",
                    spec.name,
                    b.mean,
                    c.mean,
                    100.0 * rel,
                    z
                ));
            } else {
                reject.push(format!("métrique principale `{}` : {:.4} → {:.4} (variation {:.2} %, z = {:.2}) : amélioration insuffisante ou non significative", spec.name, b.mean, c.mean, 100.0 * rel, z));
            }
        }
        _ => reject.push(format!(
            "métrique principale `{}` absente",
            policy.primary_metric
        )),
    }
    for m in metrics
        .iter()
        .filter(|m| m.protected && m.name != policy.primary_metric)
    {
        if let (Some(b), Some(c)) = (base.metrics.get(&m.name), cand.metrics.get(&m.name)) {
            let sign = if m.higher_is_better { 1.0 } else { -1.0 };
            let rel = sign * (c.mean - b.mean) / b.mean.abs().max(1e-12);
            if rel < -m.regression_tolerance {
                reject.push(format!(
                    "régression sur la métrique protégée `{}` : {:.4} → {:.4} ({:.2} %)",
                    m.name,
                    b.mean,
                    c.mean,
                    100.0 * rel
                ));
            } else {
                reasons.push(format!(
                    "métrique protégée `{}` sans régression ({:.4} → {:.4})",
                    m.name, b.mean, c.mean
                ));
            }
        }
    }
    // informative note on speed
    for m in metrics
        .iter()
        .filter(|m| matches!(m.kind, MetricKind::Speed | MetricKind::Cost))
    {
        if let (Some(b), Some(c)) = (base.metrics.get(&m.name), cand.metrics.get(&m.name)) {
            reasons.push(format!(
                "métrique de {:?} `{}` : {:.3e} → {:.3e} (informative, jamais décisive)",
                m.kind, m.name, b.mean, c.mean
            ));
        }
    }
    if reject.is_empty() {
        Decision::Accepted { reasons }
    } else {
        reject.extend(reasons.into_iter().map(|r| format!("(info) {r}")));
        Decision::Rejected { reasons: reject }
    }
}

fn evidence_rank(e: EvidenceLevel) -> u8 {
    match e {
        EvidenceLevel::UnverifiedHypothesis => 0,
        EvidenceLevel::Simulated => 1,
        EvidenceLevel::Computed => 1,
        EvidenceLevel::EmpiricallyValidated => 2,
        EvidenceLevel::MathematicallyProven => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite::MetricValue;
    use indexmap::IndexMap;

    fn eval(acc: f64, lat: f64) -> Evaluation {
        let mut m = IndexMap::new();
        m.insert(
            "accuracy".into(),
            MetricValue::of(&[acc, acc + 0.01, acc - 0.01]),
        );
        m.insert("latency".into(), MetricValue::of(&[lat]));
        Evaluation {
            candidate: CandidateId::new(),
            suite_hash: "h".into(),
            metrics: m,
            raw: IndexMap::new(),
            run_ids: vec![],
            validation: None,
            evidence: EvidenceLevel::Computed,
            failures: vec![],
            wall_time_s: 0.0,
        }
    }

    fn specs() -> Vec<MetricSpec> {
        vec![
            MetricSpec {
                name: "accuracy".into(),
                output: "accuracy".into(),
                kind: MetricKind::Quality,
                higher_is_better: true,
                protected: true,
                regression_tolerance: 0.0,
            },
            MetricSpec {
                name: "latency".into(),
                output: "latency".into(),
                kind: MetricKind::Speed,
                higher_is_better: false,
                protected: false,
                regression_tolerance: 0.0,
            },
        ]
    }

    #[test]
    fn speed_cannot_be_primary_and_faster_but_worse_is_rejected() {
        let bad = AcceptancePolicy {
            primary_metric: "latency".into(),
            min_relative_improvement: 0.0,
            min_z_score: 0.0,
            min_evidence: EvidenceLevel::Computed,
        };
        assert!(bad.validate(&specs()).is_err());
        let p = AcceptancePolicy {
            primary_metric: "accuracy".into(),
            min_relative_improvement: 0.01,
            min_z_score: 2.0,
            min_evidence: EvidenceLevel::Computed,
        };
        p.validate(&specs()).unwrap();
        let d = decide(&p, &specs(), &eval(0.9, 1.0), &eval(0.7, 0.1));
        assert!(!d.accepted());
        let d = decide(&p, &specs(), &eval(0.7, 1.0), &eval(0.9, 2.0));
        assert!(d.accepted(), "{d:?}");
    }
}
