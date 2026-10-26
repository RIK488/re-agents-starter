//! Evidence classification: the level the validator grants.

use crate::record::{CheckResult, ValidationRecord, ValidationTarget};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{EvidenceLevel, TaskResult, ValidationId};

/// Where a reference case comes from.  Only empirical references can turn a
/// simulated result into an empirically validated one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceOrigin {
    /// Closed-form / analytic solution.
    Analytic,
    /// Another numerical code (code-to-code comparison).
    Numerical,
    /// Measured / observed data.
    Empirical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Prover {
    Lean4,
    Coq,
    Isabelle,
    Z3,
    Other,
}

/// A formal proof attached to a claim.  SIMORPH records it but does **not**
/// yet machine-check certificates; the record says so explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProofArtifact {
    pub prover: Prover,
    pub statement: String,
    pub certificate_hash: String,
    /// True only when a trusted external checker verified the certificate
    /// and that verification is itself recorded in provenance.
    #[serde(default)]
    pub externally_checked: bool,
}

/// Classify a result.  Rules:
/// * any failed check ⇒ the verdict is `UnverifiedHypothesis` (the result is
///   not trusted until the failure is understood);
/// * all checks pass and at least one *empirical* reference comparison
///   passed ⇒ `EmpiricallyValidated`;
/// * all checks pass with analytic/numerical references only ⇒ the claimed
///   level, capped at `Simulated`/`Computed` (never raised above what the
///   producer claimed);
/// * `MathematicallyProven` is granted only with a `ProofArtifact` whose
///   certificate was externally checked; an unchecked artifact is recorded
///   with an explicit caveat.
pub fn classify(
    target: ValidationTarget,
    result: &TaskResult,
    checks: Vec<CheckResult>,
    reference_origins: &[ReferenceOrigin],
    proof: Option<&ProofArtifact>,
    protected_reference_hash: Option<String>,
) -> ValidationRecord {
    let claimed = result.evidence;
    let mut rationale = vec![];
    let all_pass = checks.iter().all(|c| c.passed);
    let n_fail = checks.iter().filter(|c| !c.passed).count();
    let empirical_pass = checks
        .iter()
        .any(|c| c.passed && c.name.starts_with("reference_case:"))
        && reference_origins.contains(&ReferenceOrigin::Empirical);

    let verdict = if !all_pass {
        rationale.push(format!(
            "{n_fail} contrôle(s) en échec : le résultat est rétrogradé en hypothèse non vérifiée"
        ));
        EvidenceLevel::UnverifiedHypothesis
    } else if let Some(p) = proof {
        if p.externally_checked {
            rationale.push(format!(
                "preuve {:?} vérifiée par un vérificateur externe (certificat {})",
                p.prover, p.certificate_hash
            ));
            EvidenceLevel::MathematicallyProven
        } else {
            rationale.push(format!(
                "artefact de preuve {:?} présent mais non vérifié mécaniquement par SIMORPH (certificat {}) : niveau non relevé",
                p.prover, p.certificate_hash
            ));
            cap(claimed, empirical_pass, &mut rationale)
        }
    } else {
        cap(claimed, empirical_pass, &mut rationale)
    };
    ValidationRecord {
        id: ValidationId::new(),
        target,
        checks,
        claimed,
        verdict,
        rationale,
        timestamp: Utc::now(),
        protected_reference_hash,
    }
}

fn cap(claimed: EvidenceLevel, empirical_pass: bool, rationale: &mut Vec<String>) -> EvidenceLevel {
    match claimed {
        EvidenceLevel::MathematicallyProven => {
            rationale.push("le producteur revendique une démonstration sans artefact de preuve : ramené à résultat calculé".into());
            EvidenceLevel::Computed
        }
        EvidenceLevel::EmpiricallyValidated if !empirical_pass => {
            rationale.push("validation empirique revendiquée sans référence empirique réussie : ramené à résultat simulé".into());
            EvidenceLevel::Simulated
        }
        EvidenceLevel::UnverifiedHypothesis => {
            rationale.push(
                "hypothèse non vérifiée : aucun contrôle ne peut la relever sans exécution".into(),
            );
            EvidenceLevel::UnverifiedHypothesis
        }
        level => {
            if empirical_pass
                && matches!(
                    level,
                    EvidenceLevel::Computed
                        | EvidenceLevel::Simulated
                        | EvidenceLevel::EmpiricallyValidated
                )
            {
                rationale.push("comparaison réussie à des références empiriques : résultat empiriquement validé".into());
                EvidenceLevel::EmpiricallyValidated
            } else {
                rationale.push(format!("tous les contrôles passent ; références analytiques/numériques seulement : niveau conservé ({})", level.label_fr()));
                level
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::{ComponentId, TaskRequest};

    fn res(level: EvidenceLevel) -> TaskResult {
        TaskResult::new(&TaskRequest::new(ComponentId::new()), level)
    }
    fn target() -> ValidationTarget {
        ValidationTarget::Claim {
            statement: "x".into(),
        }
    }

    #[test]
    fn never_proven_without_checked_proof() {
        let r = classify(
            target(),
            &res(EvidenceLevel::MathematicallyProven),
            vec![CheckResult::pass("a", "")],
            &[],
            None,
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Computed);
        let proof = ProofArtifact {
            prover: Prover::Lean4,
            statement: "s".into(),
            certificate_hash: "h".into(),
            externally_checked: false,
        };
        let r2 = classify(
            target(),
            &res(EvidenceLevel::Computed),
            vec![CheckResult::pass("a", "")],
            &[],
            Some(&proof),
            None,
        );
        assert_eq!(r2.verdict, EvidenceLevel::Computed);
        assert!(r2
            .rationale
            .iter()
            .any(|s| s.contains("non vérifié mécaniquement")));
        let checked = ProofArtifact {
            externally_checked: true,
            ..proof
        };
        let r3 = classify(
            target(),
            &res(EvidenceLevel::Computed),
            vec![CheckResult::pass("a", "")],
            &[],
            Some(&checked),
            None,
        );
        assert_eq!(r3.verdict, EvidenceLevel::MathematicallyProven);
    }

    #[test]
    fn empirical_only_with_empirical_reference() {
        let checks = vec![CheckResult::pass("reference_case:lab", "")];
        let a = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            checks.clone(),
            &[ReferenceOrigin::Analytic],
            None,
            None,
        );
        assert_eq!(a.verdict, EvidenceLevel::Simulated);
        let e = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            checks,
            &[ReferenceOrigin::Empirical],
            None,
            Some("abc".into()),
        );
        assert_eq!(e.verdict, EvidenceLevel::EmpiricallyValidated);
        assert!(e.to_markdown().contains("abc"));
        let claimed = classify(
            target(),
            &res(EvidenceLevel::EmpiricallyValidated),
            vec![CheckResult::pass("x", "")],
            &[],
            None,
            None,
        );
        assert_eq!(claimed.verdict, EvidenceLevel::Simulated);
    }

    #[test]
    fn failed_check_downgrades() {
        let r = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            vec![CheckResult::pass("a", ""), CheckResult::fail("b", "nope")],
            &[ReferenceOrigin::Empirical],
            None,
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::UnverifiedHypothesis);
        assert_eq!(r.failed_checks().len(), 1);
    }
}
