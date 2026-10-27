//! # Evaluation service (ARC-10, REQ-AIL-1/3/4, AC-AIL-2)
//!
//! * [`SuiteRegistry`] – sealed evaluation suites kept under a
//!   `ProtectedGuard`: adding or replacing a suite needs a human
//!   administrator **and** a signed `HumanApproval`; autonomous agents are
//!   refused and every decision is written to the audit log.  Suites are
//!   persisted as JSON and their seals verified on load.
//! * [`EvaluationService`] – evaluates a candidate on a registered suite
//!   through the AI laboratory code path and returns a [`ScoreCard`] whose
//!   sections separate quality / robustness / generalisation from speed and
//!   cost (the latter are "informatif, jamais décisif").
//! * [`standard_suites`] – sealed suites built from `plugins/ai`.
//! * [`harness`] – launcher for external evaluation harnesses (Inspect AI,
//!   LM Evaluation Harness, LightEval, HELM) through a subprocess that
//!   emits JSON results; the harnesses themselves are not bundled.

pub mod harness;
pub mod registry;
pub mod scorecard;
pub mod service;
pub mod standard;

pub use harness::*;
pub use registry::*;
pub use scorecard::*;
pub use service::*;
pub use standard::*;
