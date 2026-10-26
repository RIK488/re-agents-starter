//! # Scientific director (ARC-02)
//!
//! The director turns a [`ProblemSpec`] into a traceable research campaign:
//!
//! 1. **Analysis** ([`analysis`]): classify every quantity of the problem into
//!    the categories required by §2 of the specification (known, unknown,
//!    directly computable, requires modelling, simulable, requires data,
//!    requires proof, requires experimental verification), identify the
//!    disciplines involved and the candidate components.
//! 2. **Hypotheses** ([`hypotheses`]): formulate falsifiable hypotheses from
//!    the analysis (rule-based; an optional [`reasoner::Reasoner`] may add
//!    more, but can never change evidence levels or success criteria).
//! 3. **Programme** ([`programme`]): build an ordered experimental programme
//!    with success criteria fixed *before* execution, scored with the
//!    owner-declared trade-off weights.
//! 4. **Campaign** ([`campaign`]): the state machine that executes the
//!    programme through an [`campaign::ExperimentExecutor`], revises
//!    hypotheses from *validated* results, detects contradictions, decides the
//!    next experiment by information value, applies stopping rules, and
//!    produces conclusions that reference runs and validation records.
//!
//! Independence guarantee: this crate depends only on `simorph-core` and
//! `simorph-units`.  Validation, protected references and trade-off weights are
//! supplied from outside and are read-only here.

pub mod analysis;
pub mod campaign;
pub mod catalog;
pub mod hypotheses;
pub mod programme;
pub mod reasoner;

pub use analysis::*;
pub use campaign::*;
pub use catalog::*;
pub use hypotheses::*;
pub use programme::*;
pub use reasoner::*;
