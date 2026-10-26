//! # simorph-validation
//!
//! Independent verification engine (§3.8).  It never produces results; it
//! only judges them.  Its outputs are [`ValidationRecord`]s whose verdict is
//! the [`EvidenceLevel`](simorph_core::EvidenceLevel) the validator
//! *grants*, which may be lower than what the producing component claimed
//! and is never higher than what the evidence supports.
//!
//! Modules:
//! * [`expr`] – safe arithmetic/comparison expression evaluator used for
//!   invariants and verification criteria.
//! * [`checks`] – the individual checks (dimensional, invariants, numerical
//!   error, convergence, reproducibility, sensitivity, statistics,
//!   reference cases, generalisation).
//! * [`classify`] – evidence classification and proof artifacts.
//! * [`protected`] – sealed reference sets the director cannot alter.
//! * [`record`] – [`ValidationRecord`] / [`CheckResult`].

pub mod checks;
pub mod classify;
pub mod expr;
pub mod protected;
pub mod record;

pub use checks::*;
pub use classify::{classify, ProofArtifact, Prover, ReferenceOrigin};
pub use expr::{evaluate, Value};
pub use protected::ProtectedReferences;
pub use record::{CheckResult, ValidationRecord, ValidationTarget};
