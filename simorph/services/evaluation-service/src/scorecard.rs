//! Score cards: metric kinds kept apart.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::EvidenceLevel;
use simorph_svc_ai_laboratory::{Evaluation, EvaluationSuite, MetricKind, MetricValue};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScoreCard {
    pub candidate: String,
    pub suite: String,
    pub suite_hash: String,
    pub evidence: EvidenceLevel,
    /// Decisive sections.
    pub quality: IndexMap<String, MetricValue>,
    pub robustness: IndexMap<String, MetricValue>,
    pub generalisation: IndexMap<String, MetricValue>,
    /// Informative sections: never decide acceptance (REQ-AIL-4).
    pub speed: IndexMap<String, MetricValue>,
    pub cost: IndexMap<String, MetricValue>,
    pub failures: Vec<String>,
}

impl Default for ScoreCard {
    fn default() -> Self {
        Self {
            candidate: String::new(),
            suite: String::new(),
            suite_hash: String::new(),
            evidence: EvidenceLevel::UnverifiedHypothesis,
            quality: IndexMap::new(),
            robustness: IndexMap::new(),
            generalisation: IndexMap::new(),
            speed: IndexMap::new(),
            cost: IndexMap::new(),
            failures: vec![],
        }
    }
}

impl ScoreCard {
    pub fn from_evaluation(
        candidate_name: &str,
        suite: &EvaluationSuite,
        suite_hash: &str,
        eval: &Evaluation,
    ) -> Self {
        let mut card = ScoreCard {
            candidate: candidate_name.into(),
            suite: suite.name.clone(),
            suite_hash: suite_hash.into(),
            evidence: eval.evidence,
            failures: eval.failures.clone(),
            ..Default::default()
        };
        for m in &suite.metrics {
            let Some(v) = eval.metrics.get(&m.name) else {
                continue;
            };
            let section = match m.kind {
                MetricKind::Quality => &mut card.quality,
                MetricKind::Robustness => &mut card.robustness,
                MetricKind::Generalisation => &mut card.generalisation,
                MetricKind::Speed => &mut card.speed,
                MetricKind::Cost => &mut card.cost,
            };
            section.insert(m.name.clone(), v.clone());
        }
        card
    }

    pub fn to_markdown(&self) -> String {
        let mut s = format!(
            "# Fiche d'évaluation — {} sur `{}`\n\n",
            self.candidate, self.suite
        );
        s.push_str(&format!(
            "Suite scellée : `{}` — niveau de preuve accordé : **{}**\n\n",
            self.suite_hash,
            self.evidence.label_fr()
        ));
        let table = |s: &mut String, title: &str, note: &str, m: &IndexMap<String, MetricValue>| {
            if m.is_empty() {
                return;
            }
            s.push_str(&format!("## {title}{note}\n\n| Métrique | Moyenne | Écart-type | n | min | max |\n|---|---|---|---|---|---|\n"));
            for (k, v) in m {
                s.push_str(&format!(
                    "| {k} | {:.6e} | {:.3e} | {} | {:.6e} | {:.6e} |\n",
                    v.mean, v.std, v.n, v.min, v.max
                ));
            }
            s.push('\n');
        };
        table(
            &mut s,
            "Qualité / raisonnement",
            " (décisif)",
            &self.quality,
        );
        table(&mut s, "Robustesse", " (décisif)", &self.robustness);
        table(&mut s, "Généralisation", " (décisif)", &self.generalisation);
        table(
            &mut s,
            "Vitesse",
            " (informatif, jamais décisif)",
            &self.speed,
        );
        table(&mut s, "Coût", " (informatif, jamais décisif)", &self.cost);
        if !self.failures.is_empty() {
            s.push_str("## Échecs d'exécution\n\n");
            for f in &self.failures {
                s.push_str(&format!("- {f}\n"));
            }
        }
        s
    }
}
