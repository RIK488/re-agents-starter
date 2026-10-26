//! Provenance records.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::hashing::sha256_hex;
use simorph_core::TypedId;

/// Marker for provenance records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordMarker;
pub type RecordId = TypedId<RecordMarker>;

/// Kind of node in the provenance graph (§11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    Problem,
    Hypothesis,
    Model,
    Component,
    Experiment,
    Run,
    Dataset,
    Validation,
    Decision,
    Improvement,
    Failure,
    Correction,
    ArchitectureDecision,
    Environment,
    Knowledge,
}

impl ProvenanceKind {
    pub fn label_fr(self) -> &'static str {
        match self {
            ProvenanceKind::Problem => "problème",
            ProvenanceKind::Hypothesis => "hypothèse",
            ProvenanceKind::Model => "modèle",
            ProvenanceKind::Component => "composant",
            ProvenanceKind::Experiment => "expérience",
            ProvenanceKind::Run => "exécution",
            ProvenanceKind::Dataset => "jeu de données",
            ProvenanceKind::Validation => "validation",
            ProvenanceKind::Decision => "décision",
            ProvenanceKind::Improvement => "amélioration mesurée",
            ProvenanceKind::Failure => "échec",
            ProvenanceKind::Correction => "correction",
            ProvenanceKind::ArchitectureDecision => "décision d'architecture",
            ProvenanceKind::Environment => "environnement d'exécution",
            ProvenanceKind::Knowledge => "connaissance",
        }
    }
}

/// One immutable node of the ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceRecord {
    pub id: RecordId,
    /// Position in the chain (0-based).
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: ProvenanceKind,
    /// Short human-readable label.
    pub label: String,
    /// Arbitrary JSON payload (the full object being recorded).
    pub payload: serde_json::Value,
    /// Records this one derives from.
    #[serde(default)]
    pub parents: Vec<RecordId>,
    /// Hash of the previous record in the chain (`None` for the genesis).
    pub previous_hash: Option<String>,
    /// SHA-256 over (id, sequence, timestamp, kind, label, payload, parents, previous_hash).
    pub content_hash: String,
}

impl ProvenanceRecord {
    /// Compute the hash that `content_hash` must hold.
    pub fn compute_hash(&self) -> String {
        #[derive(Serialize)]
        struct Body<'a> {
            id: &'a RecordId,
            sequence: u64,
            timestamp: &'a DateTime<Utc>,
            kind: &'a ProvenanceKind,
            label: &'a str,
            payload: &'a serde_json::Value,
            parents: &'a [RecordId],
            previous_hash: &'a Option<String>,
        }
        let body = Body {
            id: &self.id,
            sequence: self.sequence,
            timestamp: &self.timestamp,
            kind: &self.kind,
            label: &self.label,
            payload: &self.payload,
            parents: &self.parents,
            previous_hash: &self.previous_hash,
        };
        let bytes = serde_json::to_vec(&body).expect("record is serialisable");
        sha256_hex(&bytes)
    }

    /// True if the stored hash matches the content.
    pub fn is_intact(&self) -> bool {
        self.content_hash == self.compute_hash()
    }
}
