//! # simorph-knowledge
//!
//! Scientific memory (§11) and interdisciplinary transfer (§9).
//!
//! The [`KnowledgeGraph`] stores typed nodes (concepts, methods, domains,
//! models, datasets, results, failures, decisions, improvements) with an
//! epistemic [`EvidenceLevel`](simorph_core::EvidenceLevel) and a
//! [`KnowledgeStatus`], linked by typed, weighted edges that cite the
//! provenance record justifying them.
//!
//! `suggest_transfers` proposes methods from other domains as
//! [`TransferHypothesis`] objects.  **An analogy is never a proof**: every
//! suggestion is a hypothesis with a falsification criterion, and nothing
//! in this crate can mark it established.

pub mod graph;
pub mod transfer;

pub use graph::{Edge, EdgeKind, KnowledgeGraph, KnowledgeStatus, Node, NodeId, NodeKind};
pub use transfer::{record_transfer, suggest_transfers, TransferHypothesis};
