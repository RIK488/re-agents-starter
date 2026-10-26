//! Rule-based hypothesis formulation and revision.

use crate::analysis::{ProblemAnalysis, QuantityCategory};
use simorph_core::*;

/// Formulate hypotheses from the analysis.  Every hypothesis carries a
/// falsification criterion expressed on observable outputs.
pub fn formulate(problem: &ProblemSpec, analysis: &ProblemAnalysis) -> Vec<Hypothesis> {
    let mut hs: Vec<Hypothesis> = problem.hypotheses.clone();
    for a in &analysis.assessments {
        if a.categories.contains(&QuantityCategory::Simulable)
            || a.categories.contains(&QuantityCategory::DirectlyComputable)
        {
            for cid in &a.candidate_components {
                let comp = analysis
                    .solution_candidates
                    .iter()
                    .find(|c| &c.component == cid);
                let cname = comp
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| cid.to_string());
                hs.push(Hypothesis {
                    id: HypothesisId::deterministic(&format!("{}:{}:{}", problem.id, a.name, cid)),
                    statement: format!("Le composant {cname} produit `{}` [{}] à l'intérieur de son domaine de validité pour les valeurs connues du problème.", a.name, a.unit),
                    falsification_criterion: format!("Un run de {cname} renvoie une erreur de domaine de validité, un NaN, ou échoue un cas de référence / invariant déclaré pour `{}`.", a.name),
                    status: HypothesisStatus::Open,
                    notes: comp.map(|c| c.assumptions.clone()).unwrap_or_default(),
                });
            }
        }
    }
    for c in &problem.constraints {
        hs.push(Hypothesis {
            id: HypothesisId::deterministic(&format!("{}:constraint:{}", problem.id, c.name)),
            statement: format!("La contrainte « {} » ({} {:?} {} [{}]) est satisfaisable dans l'espace de conception.", c.name, c.variable, c.kind, c.bound, c.unit),
            falsification_criterion: "Aucun point du plan d'expériences ne satisfait la contrainte avec la tolérance déclarée.".into(),
            status: HypothesisStatus::Open,
            notes: vec![],
        });
    }
    hs
}

/// Evidence collected against a hypothesis.
#[derive(Debug, Clone)]
pub struct HypothesisEvidence {
    pub hypothesis: HypothesisId,
    /// Number of validated runs consistent with the hypothesis.
    pub consistent: u32,
    /// Number of validated runs contradicting it.
    pub contradicting: u32,
    pub notes: Vec<String>,
}

/// Revise a hypothesis status from validated evidence only.
pub fn revise(h: &mut Hypothesis, ev: &HypothesisEvidence) {
    h.notes.extend(ev.notes.iter().cloned());
    h.status = match (ev.consistent, ev.contradicting) {
        (0, 0) => HypothesisStatus::Open,
        (_, 0) => HypothesisStatus::Supported,
        (0, _) => HypothesisStatus::Refuted,
        (c, k) if k as f64 / (c + k) as f64 > 0.2 => HypothesisStatus::Refuted,
        _ => HypothesisStatus::Inconclusive,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_rules() {
        let mut h = Hypothesis {
            id: HypothesisId::new(),
            statement: "s".into(),
            falsification_criterion: "f".into(),
            status: HypothesisStatus::Open,
            notes: vec![],
        };
        let id = h.id;
        revise(
            &mut h,
            &HypothesisEvidence {
                hypothesis: id,
                consistent: 5,
                contradicting: 0,
                notes: vec![],
            },
        );
        assert_eq!(h.status, HypothesisStatus::Supported);
        let id = h.id;
        revise(
            &mut h,
            &HypothesisEvidence {
                hypothesis: id,
                consistent: 0,
                contradicting: 1,
                notes: vec![],
            },
        );
        assert_eq!(h.status, HypothesisStatus::Refuted);
        let id = h.id;
        revise(
            &mut h,
            &HypothesisEvidence {
                hypothesis: id,
                consistent: 19,
                contradicting: 1,
                notes: vec![],
            },
        );
        assert_eq!(h.status, HypothesisStatus::Inconclusive);
    }
}
