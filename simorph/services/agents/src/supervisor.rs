//! Supervisor: message bus, routing, budgets, loop/deadlock detection,
//! trace persistence and deterministic replay.

use crate::agents::*;
use crate::context::*;
use crate::message::*;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_plugin_sdk::ComponentSet;
use simorph_provenance::{JsonlStore, Ledger, ProvenanceKind};
use simorph_security::{perm_set, AuditLog, Principal};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub workdir: PathBuf,
    /// Maximum number of routed messages.
    pub max_steps: usize,
    pub max_reformulations: usize,
    /// Identical (from, to, kind, digest) messages tolerated before a loop is declared.
    pub loop_threshold: usize,
    pub user_model: Option<UserModel>,
}

impl OrchestratorConfig {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            workdir: workdir.into(),
            max_steps: 64,
            max_reformulations: 1,
            loop_threshold: 3,
            user_model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEnd {
    Completed,
    Blocked { reason: String },
    StepBudgetExhausted,
    LoopDetected { message: String },
    Deadlock,
    Error { message: String },
}

pub struct SessionResult {
    pub end: SessionEnd,
    pub trace: Vec<AgentMessage>,
    pub report_markdown: String,
    pub report_path: PathBuf,
    pub trace_path: PathBuf,
    pub board: Blackboard,
    pub verdicts: Vec<VerdictRecord>,
    pub audit: AuditLog,
    pub ledger: Ledger,
}

/// Default component set (all plugins).
pub fn default_component_set() -> Result<ComponentSet> {
    let set = ComponentSet::new();
    for c in simorph_plugin_physics::components() {
        set.register_instance(Arc::from(c))?;
    }
    for m in simorph_plugin_engineering::manifests() {
        set.register_instance(Arc::from(
            simorph_plugin_engineering::build(m.id).expect("buildable"),
        ))?;
    }
    for m in simorph_plugin_ai::manifests() {
        set.register_instance(Arc::from(
            simorph_plugin_ai::build(m.id).expect("buildable"),
        ))?;
    }
    Ok(set)
}

pub struct Orchestrator {
    agents: HashMap<AgentRole, Box<dyn Agent>>,
    pub config: OrchestratorConfig,
    pub components: ComponentSet,
    observers: broadcast::Sender<AgentMessage>,
}

impl Orchestrator {
    /// Orchestrator with the standard seven agents.
    pub fn new(config: OrchestratorConfig) -> Result<Self> {
        let mut agents: HashMap<AgentRole, Box<dyn Agent>> = HashMap::new();
        for a in [
            Box::new(InterpreterAgent) as Box<dyn Agent>,
            Box::new(PlannerAgent),
            Box::new(ModelerAgent::default()),
            Box::new(ExperimenterAgent),
            Box::new(CriticAgent::default()),
            Box::new(ValidatorAgent),
            Box::new(ArchivistAgent),
        ] {
            agents.insert(a.role(), a);
        }
        let (tx, _) = broadcast::channel(1024);
        Ok(Self {
            agents,
            config,
            components: default_component_set()?,
            observers: tx,
        })
    }

    /// Replace an agent (tests, LLM-backed variants).  The replacement keeps
    /// the role's permissions — it cannot gain more.
    pub fn with_agent(mut self, agent: Box<dyn Agent>) -> Self {
        self.agents.insert(agent.role(), agent);
        self
    }

    /// Observers receive every routed message (UI, monitoring).
    pub fn subscribe(&self) -> broadcast::Receiver<AgentMessage> {
        self.observers.subscribe()
    }

    pub fn run_blocking(self, start: MessageBody) -> Result<SessionResult> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(self.run(start))
    }

    pub async fn run(mut self, start: MessageBody) -> Result<SessionResult> {
        std::fs::create_dir_all(&self.config.workdir)?;
        let ledger = Ledger::new(Box::new(JsonlStore::open(
            self.config.workdir.join("provenance.jsonl"),
        )?));
        let audit = AuditLog::with_file(self.config.workdir.join("audit.jsonl"));
        let mut ctx = AgentContext::new(
            self.config.workdir.clone(),
            self.components.clone(),
            ledger.clone(),
            audit.clone(),
        );
        ctx.max_reformulations = self.config.max_reformulations;
        ctx.board.user_model = self.config.user_model.clone();
        let ctl = ExecutionControl::default();
        let (tx, mut rx) = mpsc::unbounded_channel::<AgentMessage>();
        let mut seq = 0u64;
        let mut trace: Vec<AgentMessage> = vec![];
        let mut pending = 0usize;
        let first_to = match &start {
            MessageBody::Start { text: Some(_), .. } => AgentRole::Interpreter,
            _ => AgentRole::Interpreter,
        };
        let envelope = |seq: u64, from, to, caused_by, body: MessageBody| AgentMessage {
            seq,
            from,
            to,
            caused_by,
            timestamp: chrono::Utc::now(),
            digest: digest_of(&body),
            body,
        };
        tx.send(envelope(seq, AgentRole::User, first_to, None, start))
            .ok();
        seq += 1;
        pending += 1;
        let mut seen: BTreeMap<(AgentRole, AgentRole, &'static str, String), usize> =
            BTreeMap::new();
        let mut end: Option<SessionEnd> = None;
        let mut steps = 0usize;
        while let Some(msg) = rx.recv().await {
            pending -= 1;
            steps += 1;
            // sender authorisation for the message kind
            if !msg.body.allowed_senders().contains(&msg.from) {
                let principal = Principal::agent(msg.from.name());
                let reason = format!(
                    "{} n'est pas autorisé à émettre « {} »",
                    msg.from.name(),
                    msg.body.tag()
                );
                // forging a verdict or a final report is an attempt to alter protected state
                audit.record(
                    &principal,
                    &perm_set([Permission::SelfModify]),
                    false,
                    &reason,
                );
                let refused = envelope(
                    seq,
                    AgentRole::Supervisor,
                    AgentRole::Supervisor,
                    Some(msg.seq),
                    MessageBody::Refused {
                        offender: msg.from,
                        attempted: msg.body.tag().into(),
                        reason,
                    },
                );
                seq += 1;
                self.record(&ledger, &mut trace, msg.clone(), false);
                self.record(&ledger, &mut trace, refused, true);
                if pending == 0 && end.is_none() {
                    end = Some(SessionEnd::Deadlock);
                    break;
                }
                continue;
            }
            self.record(&ledger, &mut trace, msg.clone(), true);
            if steps > self.config.max_steps {
                end = Some(SessionEnd::StepBudgetExhausted);
                break;
            }
            let key = (msg.from, msg.to, msg.body.tag(), msg.digest.clone());
            let count = seen.entry(key).or_insert(0);
            *count += 1;
            if *count > self.config.loop_threshold {
                end = Some(SessionEnd::LoopDetected {
                    message: format!(
                        "{} → {} « {} » répété {} fois",
                        msg.from.name(),
                        msg.to.name(),
                        msg.body.tag(),
                        count
                    ),
                });
                break;
            }
            if msg.to == AgentRole::Supervisor {
                match &msg.body {
                    MessageBody::Archived { .. } => {
                        end = Some(SessionEnd::Completed);
                        break;
                    }
                    MessageBody::Blocked { reason } => {
                        ctx.board.blocked = Some(reason.clone());
                        end = Some(SessionEnd::Blocked {
                            reason: reason.clone(),
                        });
                        break;
                    }
                    _ => {
                        if pending == 0 {
                            end = Some(SessionEnd::Deadlock);
                            break;
                        }
                        continue;
                    }
                }
            }
            let Some(mut agent) = self.agents.remove(&msg.to) else {
                end = Some(SessionEnd::Error {
                    message: format!("aucun agent pour le rôle {}", msg.to.name()),
                });
                break;
            };
            let m2 = msg.clone();
            let c2 = ctl.clone();
            let (agent_back, ctx_back, res) = tokio::task::spawn_blocking(move || {
                let mut ctx = ctx;
                let r = agent.handle(&m2, &mut ctx, &c2);
                (agent, ctx, r)
            })
            .await
            .map_err(|e| SimorphError::Other(format!("agent task panicked: {e}")))?;
            ctx = ctx_back;
            let role = agent_back.role();
            self.agents.insert(role, agent_back);
            let outs = match res {
                Ok(o) => o,
                Err(e) => vec![Outgoing::to(
                    AgentRole::Supervisor,
                    MessageBody::Blocked {
                        reason: format!("{} : {e}", role.name()),
                    },
                )],
            };
            for o in outs {
                tx.send(envelope(seq, role, o.to, Some(msg.seq), o.body))
                    .ok();
                seq += 1;
                pending += 1;
            }
            if pending == 0 {
                end = Some(SessionEnd::Deadlock);
                break;
            }
        }
        let end = end.unwrap_or(SessionEnd::Deadlock);
        let report = crate::report::render(&ctx, &trace, &end, &audit);
        let report_path = self.config.workdir.join("report.md");
        std::fs::write(&report_path, &report)?;
        let final_body = MessageBody::Final {
            report_path: "report.md".into(),
            summary: format!("{end:?}"),
        };
        let fin = envelope(
            seq,
            AgentRole::Supervisor,
            AgentRole::User,
            trace.last().map(|m| m.seq),
            final_body,
        );
        self.record(&ledger, &mut trace, fin, true);
        let trace_path = self.config.workdir.join("trace.jsonl");
        save_trace(&trace_path, &trace)?;
        let verdicts = ctx.verdicts().to_vec();
        Ok(SessionResult {
            end,
            trace,
            report_markdown: report,
            report_path,
            trace_path,
            board: ctx.board,
            verdicts,
            audit,
            ledger,
        })
    }

    fn record(
        &self,
        ledger: &Ledger,
        trace: &mut Vec<AgentMessage>,
        msg: AgentMessage,
        accepted: bool,
    ) {
        let _ = ledger.record(
            ProvenanceKind::Decision,
            format!(
                "message {} {} → {} : {}{}",
                msg.seq,
                msg.from.name(),
                msg.to.name(),
                msg.body.tag(),
                if accepted { "" } else { " (refusé)" }
            ),
            &msg,
            &[],
        );
        let _ = self.observers.send(msg.clone());
        trace.push(msg);
    }
}

pub fn save_trace(path: &Path, trace: &[AgentMessage]) -> Result<()> {
    let mut s = String::new();
    for m in trace {
        s.push_str(&serde_json::to_string(m)?);
        s.push('\n');
    }
    std::fs::write(path, s)?;
    Ok(())
}

pub fn load_trace(path: &Path) -> Result<Vec<AgentMessage>> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| Ok(serde_json::from_str(l)?))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayReport {
    pub messages: usize,
    pub identical: bool,
    /// First index where the replay diverged: (index, recorded, replayed).
    pub divergence: Option<(usize, String, String)>,
}

fn sig(m: &AgentMessage) -> String {
    format!(
        "{}→{} {} {}",
        m.from.name(),
        m.to.name(),
        m.body.tag(),
        m.digest
    )
}

/// Re-execute a recorded session from its first (`Start`) message and
/// compare every (from, to, kind, digest) with the recorded trace.
pub fn replay(trace_path: &Path, mut config: OrchestratorConfig) -> Result<ReplayReport> {
    let recorded = load_trace(trace_path)?;
    let start = recorded
        .first()
        .ok_or_else(|| SimorphError::InvalidInput("empty trace".into()))?
        .body
        .clone();
    config.workdir = config.workdir.join("replay");
    let _ = std::fs::remove_dir_all(&config.workdir);
    let replayed = Orchestrator::new(config)?.run_blocking(start)?.trace;
    let n = recorded.len().max(replayed.len());
    for i in 0..n {
        let a = recorded.get(i).map(sig).unwrap_or_default();
        let b = replayed.get(i).map(sig).unwrap_or_default();
        if a != b {
            return Ok(ReplayReport {
                messages: recorded.len(),
                identical: false,
                divergence: Some((i, a, b)),
            });
        }
    }
    Ok(ReplayReport {
        messages: recorded.len(),
        identical: true,
        divergence: None,
    })
}
