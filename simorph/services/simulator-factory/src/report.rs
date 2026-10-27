//! Report types.

use serde::{Deserialize, Serialize};
use simorph_core::ComponentManifest;
use simorph_provenance::RecordId;
use simorph_simulation::Solver;
use simorph_validation::ValidationRecord;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StepOutcome {
    Done,
    Skipped { reason: String },
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
    pub step: u8,
    pub name: String,
    pub outcome: StepOutcome,
    pub details: Vec<String>,
    pub provenance: Option<RecordId>,
}

impl StepReport {
    pub fn is_done(&self) -> bool {
        matches!(self.outcome, StepOutcome::Done)
    }
}

/// One row of the method comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodRow {
    pub solver: Solver,
    /// Fixed step (fixed-step methods) or initial step (adaptive).
    pub dt: f64,
    pub rtol: f64,
    /// Max relative error of the final outputs against the reference.
    pub relative_error: f64,
    pub rhs_evals: f64,
    pub wall_time_ms: f64,
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodComparison {
    pub reference: String,
    pub horizon: f64,
    pub target_relative_error: f64,
    pub rows: Vec<MethodRow>,
    pub selected: Option<Solver>,
    pub selected_dt: Option<f64>,
}

impl MethodComparison {
    pub fn markdown(&self) -> String {
        let mut s = format!("Référence : {} (horizon {} s, erreur cible {:.1e})\n\n| Méthode | dt | rtol | erreur relative | évaluations rhs | temps (ms) | statut |\n|---|---|---|---|---|---|---|\n", self.reference, self.horizon, self.target_relative_error);
        for r in &self.rows {
            s.push_str(&format!(
                "| {:?}{} | {:.3e} | {:.1e} | {:.3e} | {} | {:.1} | {} |\n",
                r.solver,
                if Some(r.solver) == self.selected {
                    " ★"
                } else {
                    ""
                },
                r.dt,
                r.rtol,
                r.relative_error,
                r.rhs_evals,
                r.wall_time_ms,
                if r.ok {
                    "ok"
                } else {
                    r.message.as_deref().unwrap_or("échec")
                }
            ));
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationReport {
    pub request_name: String,
    pub steps: Vec<StepReport>,
    /// Name under which the final component is registered in the component set.
    pub component_name: Option<String>,
    pub manifest: Option<ComponentManifest>,
    pub method_comparison: Option<MethodComparison>,
    pub validation: Option<ValidationRecord>,
    /// Path and SHA-256 of a generated Python script, if any.
    pub generated_script: Option<(String, String)>,
    pub documentation: String,
}

impl GenerationReport {
    pub fn step(&self, n: u8) -> Option<&StepReport> {
        self.steps.iter().find(|s| s.step == n)
    }
    pub fn succeeded(&self) -> bool {
        self.component_name.is_some()
            && !self
                .steps
                .iter()
                .any(|s| matches!(s.outcome, StepOutcome::Failed { .. }))
    }
    pub fn markdown(&self) -> String {
        let mut s = format!("# Fabrique de simulateurs — {}\n\n| # | Étape | Résultat | Détails |\n|---|---|---|---|\n", self.request_name);
        for st in &self.steps {
            let o = match &st.outcome {
                StepOutcome::Done => "fait".to_string(),
                StepOutcome::Skipped { reason } => format!("ignoré : {reason}"),
                StepOutcome::Failed { reason } => format!("**ÉCHEC** : {reason}"),
            };
            s.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                st.step,
                st.name,
                o,
                st.details.join("<br>").replace('|', "\\|")
            ));
        }
        if let Some(mc) = &self.method_comparison {
            s.push_str("\n## Comparaison des méthodes numériques\n\n");
            s.push_str(&mc.markdown());
        }
        if let Some(v) = &self.validation {
            s.push_str("\n## Validation\n\n");
            s.push_str(&v.to_markdown());
        }
        if !self.documentation.is_empty() {
            s.push_str("\n## Documentation du simulateur\n\n");
            s.push_str(&self.documentation);
        }
        s
    }
}
