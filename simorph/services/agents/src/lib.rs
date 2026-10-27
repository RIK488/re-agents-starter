//! # Multi-agent orchestrator (subsystem 2)
//!
//! Seven typed agents with explicit roles cooperate over a supervised
//! message bus (tokio `mpsc` for routing, `broadcast` for observers):
//!
//! | Role | Responsibility | May emit |
//! |---|---|---|
//! | Interpreter | natural language → `ProblemSpec` (problem-understanding) | `ProblemDraft` |
//! | Planner | director analysis, hypotheses, programme | `Plan` |
//! | Modeler | model assembly; calls the simulator factory when a required output has no producer and equations are supplied | `ModelReady`, `RunCampaign`, `Replan` |
//! | Experimenter | executes campaigns (campaign runtime) | `CampaignDone` |
//! | Validator | independent checks of a campaign; the only role able to record a verdict | `Verdict` |
//! | Critic | contradictions, saturated objectives, active constraints, evidence level; proposes and triggers reformulations | `Critique`, `RunCampaign`, `Archive` |
//! | Archivist | scientific memory (knowledge service) | `Archived` |
//!
//! Every agent acts as `Principal::AutonomousAgent`; every side effect goes
//! through a permission-checked [`context::AgentContext`] operation, and
//! human-gated permissions (`SelfModify`, `ExternalSideEffect`) are always
//! refused and audited.  Message kinds are restricted to their allowed
//! senders: a forged verdict is refused by the supervisor.  The supervisor
//! enforces a step budget, detects loops and deadlocks, records every
//! message in the provenance ledger and in a JSON-lines trace, and can
//! replay a trace deterministically ([`supervisor::replay`]).

pub mod agents;
pub mod context;
pub mod message;
pub mod report;
pub mod supervisor;

pub use agents::*;
pub use context::*;
pub use message::*;
pub use supervisor::*;
