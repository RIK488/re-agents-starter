//! Evaluation subsystem (14) status as an action: which external harnesses
//! are importable here, and the integrity of the standard sealed suites.
//! Nothing is reported as available unless its Python module imports.

use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_svc_evaluation_service::{harness_available, standard_suites, HarnessKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessStatus {
    pub kind: HarnessKind,
    pub module: Option<String>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteStatus {
    pub name: String,
    pub version: String,
    pub tasks: usize,
    pub metrics: usize,
    pub seal: String,
    pub seal_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationStatus {
    pub harnesses: Vec<HarnessStatus>,
    pub suites: Vec<SuiteStatus>,
}

pub fn evaluation_status() -> Result<EvaluationStatus> {
    let harnesses = [
        HarnessKind::InspectAi,
        HarnessKind::LmEvalHarness,
        HarnessKind::LightEval,
        HarnessKind::Helm,
    ]
    .into_iter()
    .map(|k| {
        let r = harness_available(k);
        HarnessStatus {
            kind: k,
            module: k.probe_module().map(String::from),
            available: r.is_ok(),
            reason: r.err(),
        }
    })
    .collect();
    let suites = standard_suites()
        .into_iter()
        .map(|s| {
            let inner = s.suite();
            SuiteStatus {
                name: inner.name.clone(),
                version: inner.version.clone(),
                tasks: inner.tasks.len(),
                metrics: inner.metrics.len(),
                seal: s.hash().to_string(),
                seal_verified: s.verify().is_ok(),
            }
        })
        .collect();
    Ok(EvaluationStatus { harnesses, suites })
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_suites_are_sealed_and_harness_detection_is_explicit() {
        let s = super::evaluation_status().unwrap();
        assert!(!s.suites.is_empty());
        assert!(s.suites.iter().all(|x| x.seal_verified));
        assert_eq!(s.harnesses.len(), 4);
        // unavailable harnesses always carry the cause
        assert!(s.harnesses.iter().all(|h| h.available || h.reason.is_some()));
    }
}
