//! Epistemic categories.
//!
//! The SIMORPH specification (§3.8) requires five **non-interchangeable**
//! categories for every result.  A value may only move to a stronger
//! category through an explicit, recorded validation step, never by
//! relabelling.

use crate::ids::{RunId, ValidationId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Epistemic status of a result or claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    /// A conjecture that has not been checked by any means.
    UnverifiedHypothesis,
    /// Obtained by direct computation from a closed-form or exact method
    /// (e.g. analytic formula, exact arithmetic, deterministic algorithm).
    Computed,
    /// Obtained by numerical simulation of a model.  Subject to model error and
    /// numerical error.
    Simulated,
    /// Compared against experimental / observational reference data within a
    /// declared tolerance.
    EmpiricallyValidated,
    /// Backed by a machine-checked or peer-reviewed mathematical proof.
    MathematicallyProven,
}

impl EvidenceLevel {
    /// Human readable French label used in reports.
    pub fn label_fr(self) -> &'static str {
        match self {
            EvidenceLevel::UnverifiedHypothesis => "hypothèse non vérifiée",
            EvidenceLevel::Computed => "résultat calculé",
            EvidenceLevel::Simulated => "résultat simulé",
            EvidenceLevel::EmpiricallyValidated => "résultat empiriquement validé",
            EvidenceLevel::MathematicallyProven => "résultat mathématiquement démontré",
        }
    }

    /// Categories are deliberately **not** totally ordered: a simulated
    /// result is not "better" than a computed one, and a proof and an
    /// empirical validation are different kinds of knowledge.  This method
    /// returns the transitions that a validation record is allowed to
    /// perform.  Anything else is refused.
    pub fn allowed_transition(from: Self, to: Self) -> bool {
        use EvidenceLevel::*;
        matches!(
            (from, to),
            (UnverifiedHypothesis, Computed)
                | (UnverifiedHypothesis, Simulated)
                | (UnverifiedHypothesis, MathematicallyProven)
                | (Computed, EmpiricallyValidated)
                | (Simulated, EmpiricallyValidated)
                | (Computed, MathematicallyProven)
        ) || from == to
    }
}

/// A claim made by SIMORPH about the world or about a model, with its
/// epistemic level and the records that support it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Claim {
    pub statement: String,
    pub level: EvidenceLevel,
    /// Runs whose outputs support the claim.
    #[serde(default)]
    pub supporting_runs: Vec<RunId>,
    /// Validation records that justified the current level.
    #[serde(default)]
    pub validations: Vec<ValidationId>,
    /// Free-form caveats (model assumptions, tolerance, data provenance).
    #[serde(default)]
    pub caveats: Vec<String>,
}

impl Claim {
    pub fn hypothesis(statement: impl Into<String>) -> Self {
        Self {
            statement: statement.into(),
            level: EvidenceLevel::UnverifiedHypothesis,
            supporting_runs: vec![],
            validations: vec![],
            caveats: vec![],
        }
    }

    /// Promote the claim to a new level.  Refused unless the transition is
    /// allowed and a validation record is supplied.
    pub fn promote(&mut self, to: EvidenceLevel, validation: ValidationId) -> crate::Result<()> {
        if !EvidenceLevel::allowed_transition(self.level, to) {
            return Err(crate::SimorphError::Validation(format!(
                "transition {:?} -> {:?} is not allowed",
                self.level, to
            )));
        }
        self.validations.push(validation);
        self.level = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_cannot_become_proven_without_proof() {
        let mut c = Claim::hypothesis("x");
        c.promote(EvidenceLevel::Simulated, ValidationId::new())
            .unwrap();
        assert!(c
            .promote(EvidenceLevel::MathematicallyProven, ValidationId::new())
            .is_err());
    }

    #[test]
    fn empirical_validation_requires_a_record() {
        let mut c = Claim::hypothesis("y");
        c.promote(EvidenceLevel::Computed, ValidationId::new())
            .unwrap();
        c.promote(EvidenceLevel::EmpiricallyValidated, ValidationId::new())
            .unwrap();
        assert_eq!(c.validations.len(), 2);
    }
}
