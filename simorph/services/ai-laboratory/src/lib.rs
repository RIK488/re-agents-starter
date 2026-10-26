//! # AI laboratory (ARC-10, §5)
//!
//! Implements the improvement loop of §5.1 for any AI system exposed as a
//! `simorph_core::Component`:
//!
//! ```text
//! initial evaluation → limits → hypotheses → experiment design → execution
//! → independent validation → knowledge → usable data → training/optimisation
//! → re-evaluation → accept / reject → iterate
//! ```
//!
//! Guarantees:
//! * Evaluation suites are **sealed** ([`SealedSuite`]): their hash is written
//!   into every evaluation and acceptance record; a candidate (or an
//!   autonomous agent) cannot alter the suite that judges it.
//! * Acceptance requires a statistically significant improvement of the
//!   *primary* metric, which must be a quality/reasoning metric, **and** no
//!   regression on protected metrics.  A faster candidate that reasons worse
//!   is rejected (REQ-AIL-4).
//! * Every accepted change is reversible ([`Laboratory::rollback`]).
//! * Self-improvement of SIMORPH (§5.2) uses the same loop on configuration
//!   candidates; code-level self-modification is gated by
//!   `Permission::SelfModify`, which this crate cannot grant.

pub mod improver;
pub mod laboratory;
pub mod policy;
pub mod suite;

pub use improver::*;
pub use laboratory::*;
pub use policy::*;
pub use suite::*;
