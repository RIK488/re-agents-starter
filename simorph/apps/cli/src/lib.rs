//! # simorph-cli (library)
//!
//! The action library shared by the CLI binary and the web API.  Every
//! public function here corresponds to a user-visible command; the list in
//! [`actions::ACTIONS`] is generated from this module and is what the web UI
//! renders, which enforces REQ-UI-13 ("every visible command is a real,
//! implemented action").

pub mod actions;
pub mod bench;
pub mod examples;
pub mod factories;
pub mod lab;
pub mod registry;
pub mod workspace;

pub use actions::*;
pub use factories::*;
pub use registry::*;
pub use workspace::*;

/// Action ids that have an implementation (kept in sync with `main.rs` and
/// the web router; checked by a test in `actions`).
pub fn implemented_action_ids() -> Vec<&'static str> {
    vec![
        "components.list",
        "components.show",
        "units.convert",
        "units.check",
        "problem.example",
        "problem.validate",
        "analysis.run",
        "programme.plan",
        "campaign.run",
        "campaign.get",
        "campaign.events",
        "run.execute",
        "experiment.list",
        "experiment.reproduce",
        "optimisation.methods",
        "uncertainty.sobol",
        "lab.run",
        "lab.suites",
        "provenance.list",
        "provenance.explain",
        "provenance.verify",
        "schemas.export",
        "bench.run",
        "actions.list",
        "model.assemble",
        "model.from_equations",
        "simulator.generate",
        "knowledge.memory",
        "knowledge.transfers",
    ]
}
