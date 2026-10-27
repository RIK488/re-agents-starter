//! Typed messages exchanged on the bus.

use serde::{Deserialize, Serialize};
use simorph_core::*;

/// Roles of the laboratory.  Each role has its own principal
/// (`Principal::agent(role)`), so permissions and audit entries are per role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    User,
    Supervisor,
    Interpreter,
    Planner,
    Modeler,
    Experimenter,
    Critic,
    Validator,
    Archivist,
}

impl AgentRole {
    pub fn name(self) -> &'static str {
        match self {
            AgentRole::User => "user",
            AgentRole::Supervisor => "supervisor",
            AgentRole::Interpreter => "interpreter",
            AgentRole::Planner => "planner",
            AgentRole::Modeler => "modeler",
            AgentRole::Experimenter => "experimenter",
            AgentRole::Critic => "critic",
            AgentRole::Validator => "validator",
            AgentRole::Archivist => "archivist",
        }
    }
}

/// Conclusion as carried on the bus (no random identifiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConclusionView {
    pub variable: String,
    pub value: f64,
    pub unit: String,
    pub level: EvidenceLevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckView {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Proposal {
    /// Run a new campaign with a reformulated problem.
    Reformulate {
        spec: ProblemSpec,
        rationale: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageBody {
    /// Session start: a natural-language text or a structured specification.
    Start {
        text: Option<String>,
        spec: Option<ProblemSpec>,
    },
    ProblemDraft {
        spec: ProblemSpec,
        mappings: usize,
        ambiguities: Vec<String>,
    },
    Plan {
        experiments: usize,
        candidates: Vec<String>,
        issues: Vec<String>,
    },
    Replan {
        reason: String,
    },
    ModelReady {
        complete: bool,
        chain: Vec<String>,
        unresolved: Vec<String>,
        generated: Option<String>,
    },
    RunCampaign {
        spec: ProblemSpec,
        reason: String,
    },
    CampaignDone {
        index: usize,
        stop_reason: String,
        conclusions: Vec<ConclusionView>,
    },
    Verdict {
        index: usize,
        passed: bool,
        checks: Vec<CheckView>,
    },
    Critique {
        index: usize,
        findings: Vec<Finding>,
        proposal: Option<Proposal>,
    },
    Archive {
        campaigns: Vec<usize>,
    },
    Archived {
        nodes_added: usize,
        edges_added: usize,
        memory_path: String,
    },
    Blocked {
        reason: String,
    },
    Refused {
        offender: AgentRole,
        attempted: String,
        reason: String,
    },
    Final {
        report_path: String,
        summary: String,
    },
}

impl MessageBody {
    pub fn tag(&self) -> &'static str {
        match self {
            MessageBody::Start { .. } => "start",
            MessageBody::ProblemDraft { .. } => "problem_draft",
            MessageBody::Plan { .. } => "plan",
            MessageBody::Replan { .. } => "replan",
            MessageBody::ModelReady { .. } => "model_ready",
            MessageBody::RunCampaign { .. } => "run_campaign",
            MessageBody::CampaignDone { .. } => "campaign_done",
            MessageBody::Verdict { .. } => "verdict",
            MessageBody::Critique { .. } => "critique",
            MessageBody::Archive { .. } => "archive",
            MessageBody::Archived { .. } => "archived",
            MessageBody::Blocked { .. } => "blocked",
            MessageBody::Refused { .. } => "refused",
            MessageBody::Final { .. } => "final",
        }
    }

    /// Roles allowed to emit this kind of message.  The supervisor refuses
    /// (and audits) anything else — e.g. only the Validator may emit a verdict.
    pub fn allowed_senders(&self) -> &'static [AgentRole] {
        use AgentRole::*;
        match self {
            MessageBody::Start { .. } => &[User],
            MessageBody::ProblemDraft { .. } => &[Interpreter],
            MessageBody::Plan { .. } => &[Planner],
            MessageBody::Replan { .. } => &[Modeler],
            MessageBody::ModelReady { .. } => &[Modeler],
            MessageBody::RunCampaign { .. } => &[Modeler, Critic],
            MessageBody::CampaignDone { .. } => &[Experimenter],
            MessageBody::Verdict { .. } => &[Validator],
            MessageBody::Critique { .. } => &[Critic],
            MessageBody::Archive { .. } => &[Critic],
            MessageBody::Archived { .. } => &[Archivist],
            MessageBody::Blocked { .. } => &[
                Interpreter,
                Planner,
                Modeler,
                Experimenter,
                Critic,
                Validator,
                Archivist,
                Supervisor,
            ],
            MessageBody::Refused { .. } => &[Supervisor],
            MessageBody::Final { .. } => &[Supervisor],
        }
    }
}

/// Envelope.  `digest` covers the body with every `id` field removed, so
/// that a deterministic replay produces identical digests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub seq: u64,
    pub from: AgentRole,
    pub to: AgentRole,
    pub caused_by: Option<u64>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub body: MessageBody,
    pub digest: String,
}

fn strip_ids(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.remove("id");
            for (_, x) in m.iter_mut() {
                strip_ids(x);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                strip_ids(x);
            }
        }
        _ => {}
    }
}

pub fn digest_of(body: &MessageBody) -> String {
    let mut v = serde_json::to_value(body).expect("serialisable");
    strip_ids(&mut v);
    simorph_core::hashing::hash_json(&v).expect("serialisable")
}

/// Message produced by an agent (the supervisor fills the envelope).
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub to: AgentRole,
    pub body: MessageBody,
}

impl Outgoing {
    pub fn to(to: AgentRole, body: MessageBody) -> Self {
        Self { to, body }
    }
}
