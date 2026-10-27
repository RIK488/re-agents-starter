//! Report of an improvement campaign (French Markdown + JSON).

use crate::campaign::{ImprovementConfig, IterationRecord, Variant};
use crate::evaluation::SuiteEvaluation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImprovementReport {
    pub target: String,
    pub simulator: String,
    pub config: ImprovementConfig,
    pub suite_hash: String,
    pub suite_size: usize,
    pub probe_size: usize,
    pub initial_dataset_size: usize,
    pub initial: SuiteEvaluation,
    pub iterations: Vec<IterationRecord>,
    pub final_evaluation: SuiteEvaluation,
    pub final_dataset_size: usize,
    pub accepted_versions: usize,
    pub simulations_used: usize,
    pub rejected_samples: usize,
    pub stop_reason: String,
    pub ledger_records: usize,
    pub wall_time_s: f64,
}

impl ImprovementReport {
    pub fn relative_rmse_gain(&self) -> f64 {
        (self.initial.rmse_log10 - self.final_evaluation.rmse_log10) / self.initial.rmse_log10
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialisable")
    }

    pub fn to_markdown(&self) -> String {
        let mut s = format!(
            "# Amélioration en boucle fermée — surrogate de `{}`\n\n",
            self.simulator
        );
        s.push_str(&format!(
            "- Système d'IA amélioré : réseau de neurones {:?} (tanh), entraînement contrôlé ({} époques, Adam, graine {})\n- Juge : suite scellée de {} points simulés et validés, empreinte `{}`\n- Variante : {}\n- Simulations consommées : {} (budget {}), échantillons rejetés par la validation : {}\n- Arrêt : {}\n- Enregistrements de provenance : {} ; durée : {:.1} s\n\n",
            self.config.hidden,
            self.config.epochs,
            self.config.seed,
            self.suite_size,
            self.suite_hash,
            variant_fr(self.config.variant),
            self.simulations_used,
            self.config.max_simulations,
            self.rejected_samples,
            self.stop_reason,
            self.ledger_records,
            self.wall_time_s
        ));
        s.push_str("| Version | Données | RMSE log10 (suite) | Erreur relative médiane | Erreur relative max | RMSE protégée | Violations de monotonie | Décision |\n|---|---|---|---|---|---|---|---|\n");
        s.push_str(&row(
            "initiale",
            self.initial_dataset_size,
            &self.initial,
            "—",
        ));
        for it in &self.iterations {
            s.push_str(&row(
                &format!("candidat {}", it.index),
                it.dataset_size,
                &it.candidate,
                if it.decision.accepted {
                    "**acceptée**"
                } else {
                    "rejetée"
                },
            ));
        }
        s.push_str(&format!(
            "\nGain relatif final de RMSE : **{:.1} %** ({} version(s) acceptée(s)).\n",
            100.0 * self.relative_rmse_gain(),
            self.accepted_versions
        ));
        s.push_str("\n## Erreur par régime physique (suite scellée)\n\n| Régime | n | RMSE initiale | RMSE finale |\n|---|---|---|---|\n");
        for (k, (n, e0)) in &self.initial.rmse_by_regime {
            let e1 = self
                .final_evaluation
                .rmse_by_regime
                .get(k)
                .map(|v| v.1)
                .unwrap_or(f64::NAN);
            s.push_str(&format!("| {k} | {n} | {e0:.4} | {e1:.4} |\n"));
        }
        for it in &self.iterations {
            s.push_str(&format!("\n## Itération {}\n\n", it.index));
            if let Some(h) = &it.hypothesis {
                s.push_str(&format!(
                    "- Hypothèse : {}\n- Critère de réfutation : {}\n",
                    h.statement, h.falsification
                ));
            }
            s.push_str(&format!("- Expérience `{}` : {} points demandés, {} validés, {} rejetés ({} simulations au total)\n", it.experiment_id, it.n_requested, it.n_validated, it.n_rejected, it.n_simulated));
            if it.strategy_selection.len() > 1 {
                s.push_str(
                    "- Sélection de stratégie (RMSE sur le sondage, jamais sur la suite scellée) :",
                );
                for (k, v) in &it.strategy_selection {
                    s.push_str(&format!(" {k} = {v:.4} ;"));
                }
                s.push('\n');
            }
            for r in &it.rejection_examples {
                s.push_str(&format!("  - rejet : {r}\n"));
            }
            s.push_str(&format!("- Jeu de données `{}` ({} échantillons)\n- Entraînement : {} époques, perte finale {:.3e}, {:.1} s, modèle `{}`\n- Décision :\n", it.dataset_hash, it.dataset_size, it.training.epochs, it.training.final_train_loss, it.training.wall_time_s, it.training.model_hash));
            for r in &it.decision.reasons {
                s.push_str(&format!("  - {r}\n"));
            }
        }
        s
    }
}

fn variant_fr(v: Variant) -> String {
    match v {
        Variant::Targeted => "échantillonnage ciblé sur les cellules de plus forte erreur".into(),
        Variant::Uniform => "échantillonnage uniforme (référence)".into(),
        Variant::Adaptive => "adaptative : stratégie (ciblée / hybride / uniforme) choisie sur l'ensemble de sondage à chaque itération".into(),
        Variant::Hybrid { targeted_fraction } => format!("hybride : {:.0} % ciblé sur les cellules de plus forte erreur, le reste uniforme", 100.0 * targeted_fraction),
        Variant::CorruptedLabels { factor } => format!("contrôle négatif : étiquettes corrompues après validation (×{factor})"),
        Variant::BuggySimulator { factor } => format!("contrôle négatif : simulateur défectueux (×{factor}) avant validation"),
    }
}

fn row(name: &str, n: usize, e: &SuiteEvaluation, d: &str) -> String {
    format!(
        "| {name} | {n} | {:.4} | {:.2} % | {:.1} % | {:.4} | {:.3} | {d} |\n",
        e.rmse_log10,
        100.0 * e.median_relative_error,
        100.0 * e.max_relative_error,
        e.protected_rmse_log10,
        e.monotonicity_violation_rate
    )
}
