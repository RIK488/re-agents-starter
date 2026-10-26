//! # simorph-experiment
//!
//! Universal experimentation engine (§3.6).
//!
//! * [`design`] – designs of experiments (full factorial, Latin hypercube,
//!   random, one-at-a-time, central composite, Plackett–Burman).
//! * [`plan`] – [`ExperimentPlan`] / [`ExperimentRecord`] / [`RunRecord`].
//! * [`runner`] – parallel, deterministic execution through any
//!   [`simorph_core::Component`], reproduction of a past experiment.
//! * [`archive`] – JSON archive and provenance hook.
//! * [`analysis`] – counter-example search and failure-cause attribution.
//!
//! Every experiment and every run carries a unique id and the hashes needed
//! to reproduce it.

pub mod analysis;
pub mod archive;
pub mod design;
pub mod plan;
pub mod runner;

pub use analysis::{find_counter_example, rank_failure_causes, CauseScore, CounterExample};
pub use archive::{ExperimentArchive, ProvenanceHook};
pub use design::{Design, DesignKind, Factor};
pub use plan::{
    ExperimentPlan, ExperimentRecord, OutputSummary, RunRecord, RunStatus, SuccessCriterion,
};
pub use runner::{reproduce, run_experiment, ReproductionReport};
