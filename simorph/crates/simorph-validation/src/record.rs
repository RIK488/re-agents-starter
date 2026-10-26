//! Validation records.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{EvidenceLevel, ExperimentId, RunId, ValidationId};

/// What a validation record is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationTarget {
    Run { run_id: RunId },
    Experiment { experiment_id: ExperimentId },
    Claim { statement: String },
    Component { name: String, version: String },
    Model { name: String },
}

/// Outcome of one check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub details: String,
    #[serde(default)]
    pub metrics: IndexMap<String, f64>,
}

impl CheckResult {
    pub fn pass(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            details: details.into(),
            metrics: IndexMap::new(),
        }
    }
    pub fn fail(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            details: details.into(),
            metrics: IndexMap::new(),
        }
    }
    pub fn metric(mut self, k: &str, v: f64) -> Self {
        self.metrics.insert(k.into(), v);
        self
    }
}

/// A complete validation record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidationRecord {
    pub id: ValidationId,
    pub target: ValidationTarget,
    pub checks: Vec<CheckResult>,
    /// Level the producer claimed.
    pub claimed: EvidenceLevel,
    /// Level the validator grants.
    pub verdict: EvidenceLevel,
    /// Why the verdict is what it is.
    pub rationale: Vec<String>,
    pub timestamp: DateTime<Utc>,
    /// Hash of the sealed reference set used, if any.
    #[serde(default)]
    pub protected_reference_hash: Option<String>,
}

impl ValidationRecord {
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
    pub fn failed_checks(&self) -> Vec<&CheckResult> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }
    pub fn to_markdown(&self) -> String {
        let mut s = format!("# Validation {}\n\n", self.id);
        s.push_str(&format!("- Cible : {:?}\n", self.target));
        s.push_str(&format!(
            "- Niveau revendiqué : {}\n",
            self.claimed.label_fr()
        ));
        s.push_str(&format!("- **Verdict : {}**\n\n", self.verdict.label_fr()));
        s.push_str("| Contrôle | Résultat | Détails |\n|---|---|---|\n");
        for c in &self.checks {
            s.push_str(&format!(
                "| {} | {} | {} |\n",
                c.name,
                if c.passed { "✓" } else { "✗" },
                c.details.replace('|', "\\|")
            ));
        }
        s.push_str("\nJustification :\n");
        for r in &self.rationale {
            s.push_str(&format!("- {r}\n"));
        }
        if let Some(h) = &self.protected_reference_hash {
            s.push_str(&format!("\nRéférences protégées : `{h}`\n"));
        }
        s
    }
}
