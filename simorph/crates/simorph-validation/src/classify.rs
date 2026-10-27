//! Evidence classification: the level the validator grants.

use crate::record::{CheckResult, ValidationRecord, ValidationTarget};
use chrono::Utc;
use indexmap::IndexMap;
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

/// Classify a result (conservative compatibility wrapper).
///
/// `reference_origins` is not linked to individual reference cases, so the
/// result can be granted `EmpiricallyValidated` only when *every* supplied
/// origin is `Empirical` and at least one `reference_case:` check passed.
/// Use [`classify_with_origins`] to link each case to its origin.
pub fn classify(
    target: ValidationTarget,
    result: &TaskResult,
    checks: Vec<CheckResult>,
    reference_origins: &[ReferenceOrigin],
    proof: Option<&ProofArtifact>,
    protected_reference_hash: Option<String>,
) -> ValidationRecord {
    let all_empirical = !reference_origins.is_empty()
        && reference_origins
            .iter()
            .all(|o| *o == ReferenceOrigin::Empirical);
    let mut origins = IndexMap::new();
    for c in &checks {
        if let Some(case) = c.name.strip_prefix("reference_case:") {
            origins.insert(
                case.to_string(),
                if all_empirical {
                    ReferenceOrigin::Empirical
                } else {
                    reference_origins
                        .iter()
                        .copied()
                        .find(|o| *o != ReferenceOrigin::Empirical)
                        .unwrap_or(ReferenceOrigin::Analytic)
                },
            );
        }
    }
    classify_with_origins(
        target,
        result,
        checks,
        &origins,
        proof,
        protected_reference_hash,
    )
}

/// Whether `to` is a *lowering* of `from` that the validator may impose.
fn is_downgrade(from: EvidenceLevel, to: EvidenceLevel) -> bool {
    use EvidenceLevel::*;
    to == UnverifiedHypothesis
        || matches!(
            (from, to),
            (MathematicallyProven, Computed) | (EmpiricallyValidated, Simulated)
        )
}

/// Classify a result.  Rules:
/// * no check at all ⇒ nothing can be raised: the claimed level is kept,
///   except that unsupported `MathematicallyProven` / `EmpiricallyValidated`
///   claims are lowered;
/// * any failed check ⇒ `UnverifiedHypothesis`;
/// * `EmpiricallyValidated` ⇒ at least one passing `reference_case:<name>`
///   check whose case is linked to `ReferenceOrigin::Empirical` in
///   `origins`, and every `reference_case:` check is linked to an origin;
/// * `MathematicallyProven` ⇒ a `ProofArtifact` with `externally_checked`
///   AND at least one passing `formal:` check (produced by the formal
///   engine) AND the transition is allowed from the claimed level;
/// * the verdict is always either the claimed level, an allowed transition
///   (`EvidenceLevel::allowed_transition`) or a lowering.
pub fn classify_with_origins(
    target: ValidationTarget,
    result: &TaskResult,
    checks: Vec<CheckResult>,
    origins: &IndexMap<String, ReferenceOrigin>,
    proof: Option<&ProofArtifact>,
    protected_reference_hash: Option<String>,
) -> ValidationRecord {
    let claimed = result.evidence;
    let mut rationale = vec![];
    let n_fail = checks.iter().filter(|c| !c.passed).count();
    let ref_checks: Vec<&CheckResult> = checks
        .iter()
        .filter(|c| c.name.starts_with("reference_case:"))
        .collect();
    let all_linked = ref_checks
        .iter()
        .all(|c| origins.contains_key(&c.name["reference_case:".len()..]));
    let empirical_pass = all_linked
        && ref_checks.iter().any(|c| {
            c.passed
                && origins.get(&c.name["reference_case:".len()..])
                    == Some(&ReferenceOrigin::Empirical)
        });
    if !ref_checks.is_empty() && !all_linked {
        rationale.push(
            "cas de référence sans origine déclarée : aucune validation empirique possible".into(),
        );
    }
    let formal_pass = checks
        .iter()
        .any(|c| c.passed && c.name.starts_with("formal:"));

    let mut verdict = if checks.is_empty() {
        rationale.push("aucun contrôle : le niveau ne peut pas être relevé".into());
        match claimed {
            EvidenceLevel::MathematicallyProven => EvidenceLevel::Computed,
            EvidenceLevel::EmpiricallyValidated => EvidenceLevel::Simulated,
            l => l,
        }
    } else if n_fail > 0 {
        rationale.push(format!(
            "{n_fail} contrôle(s) en échec : le résultat est rétrogradé en hypothèse non vérifiée"
        ));
        EvidenceLevel::UnverifiedHypothesis
    } else if let Some(p) = proof {
        if p.externally_checked
            && formal_pass
            && EvidenceLevel::allowed_transition(claimed, EvidenceLevel::MathematicallyProven)
        {
            rationale.push(format!(
                "preuve {:?} vérifiée (contrôles formal: réussis, certificat {})",
                p.prover, p.certificate_hash
            ));
            EvidenceLevel::MathematicallyProven
        } else {
            if !p.externally_checked {
                rationale.push(format!(
                    "artefact de preuve {:?} présent mais non vérifié mécaniquement par SIMORPH (certificat {}) : niveau non relevé",
                    p.prover, p.certificate_hash
                ));
            } else if !formal_pass {
                rationale.push("artefact de preuve marqué vérifié mais aucun contrôle formal: réussi : niveau non relevé".into());
            } else {
                rationale.push(format!(
                    "transition {:?} → démontré interdite : niveau non relevé",
                    claimed
                ));
            }
            cap(claimed, empirical_pass, &mut rationale)
        }
    } else {
        cap(claimed, empirical_pass, &mut rationale)
    };
    // Final guard: never an upgrade that the core transition table forbids.
    if verdict != claimed
        && !EvidenceLevel::allowed_transition(claimed, verdict)
        && !is_downgrade(claimed, verdict)
    {
        rationale.push(format!(
            "transition {claimed:?} → {verdict:?} interdite par la table des niveaux : niveau revendiqué conservé"
        ));
        verdict = claimed;
    }
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
            vec![CheckResult::pass("formal:smt", "")],
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

    #[test]
    fn audit_bare_proof_flag_does_not_prove_simulated_result() {
        // Audit probe: externally_checked=true with zero checks turned a
        // Simulated result into MathematicallyProven.
        let proof = ProofArtifact {
            prover: Prover::Z3,
            statement: "s".into(),
            certificate_hash: "h".into(),
            externally_checked: true,
        };
        let r = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            vec![],
            &[],
            Some(&proof),
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Simulated);
        // Even with a formal check, Simulated → Proven is forbidden.
        let r = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            vec![CheckResult::pass("formal:smt", "")],
            &[],
            Some(&proof),
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Simulated);
        // A non-formal check is not enough.
        let r = classify(
            target(),
            &res(EvidenceLevel::Computed),
            vec![CheckResult::pass("invariants", "")],
            &[],
            Some(&proof),
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Computed);
    }

    #[test]
    fn audit_empirical_requires_linked_origin() {
        // Audit probe: any reference_case check + Empirical anywhere in the
        // origins list granted EmpiricallyValidated.
        let checks = vec![
            CheckResult::pass("reference_case:analytic", ""),
            CheckResult::pass("reference_case:lab", ""),
        ];
        let mixed = classify(
            target(),
            &res(EvidenceLevel::Simulated),
            checks.clone(),
            &[ReferenceOrigin::Analytic, ReferenceOrigin::Empirical],
            None,
            None,
        );
        assert_eq!(mixed.verdict, EvidenceLevel::Simulated);
        let mut origins = IndexMap::new();
        origins.insert("analytic".to_string(), ReferenceOrigin::Analytic);
        let unlinked = classify_with_origins(
            target(),
            &res(EvidenceLevel::Simulated),
            checks.clone(),
            &origins,
            None,
            None,
        );
        assert_eq!(unlinked.verdict, EvidenceLevel::Simulated);
        origins.insert("lab".to_string(), ReferenceOrigin::Empirical);
        let linked = classify_with_origins(
            target(),
            &res(EvidenceLevel::Simulated),
            checks,
            &origins,
            None,
            None,
        );
        assert_eq!(linked.verdict, EvidenceLevel::EmpiricallyValidated);
    }

    #[test]
    fn audit_zero_checks_never_raise() {
        let r = classify(
            target(),
            &res(EvidenceLevel::EmpiricallyValidated),
            vec![],
            &[ReferenceOrigin::Empirical],
            None,
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Simulated);
        let r = classify(
            target(),
            &res(EvidenceLevel::Computed),
            vec![],
            &[ReferenceOrigin::Empirical],
            None,
            None,
        );
        assert_eq!(r.verdict, EvidenceLevel::Computed);
    }
}
