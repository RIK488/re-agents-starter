//! # simorph-provenance
//!
//! Append-only, hash-chained ledger of everything SIMORPH does (§3.6, §11).
//!
//! * [`ProvenanceRecord`] – one immutable node: kind, JSON payload, parent
//!   record ids, content hash and the hash of the previous record.
//! * [`ProvenanceStore`] – trait implemented by [`MemoryStore`] and
//!   [`JsonlStore`] (JSON-lines file, verified on load).
//! * [`Ledger`] – thread-safe append API, environment capture, queries
//!   (`ancestors`, `descendants`, `explain`).
//!
//! Tampering with any stored record breaks the chain, which
//! [`Ledger::verify_chain`] detects.

pub mod environment;
pub mod ledger;
pub mod record;
pub mod store;

pub use environment::EnvironmentCapture;
pub use ledger::{Explanation, ExplanationStep, Ledger};
pub use record::{ProvenanceKind, ProvenanceRecord, RecordId};
pub use store::{JsonlStore, MemoryStore, ProvenanceStore};
