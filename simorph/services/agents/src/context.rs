//! Shared state and permission-guarded operations.
//!
//! Agents never hold the security policy, the component set or the ledger
//! directly: every side effect goes through an [`AgentContext`] method that
//! (1) checks that the calling role is allowed to perform the operation and
//! (2) asks the [`Policy`] for the corresponding permissions with the role's
//! `AutonomousAgent` principal.  Both refusals are written to the audit log.

use crate::message::{AgentRole, CheckView, Finding};
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_plugin_sdk::ComponentSet;
use simorph_provenance::{Ledger, ProvenanceKind};
use simorph_security::{perm_set, AuditLog, Policy, Principal};
use simorph_svc_scientific_director::Campaign;
use std::path::PathBuf;
use std::sync::Arc;

/// Equations supplied by the user for the simulator factory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserModel {
    pub name: String,
    pub equations: Vec<String>,
    pub variables: Vec<Port>,
    pub parameters: Vec<Quantity>,
    #[serde(default)]
    pub reference_cases: Vec<ReferenceCase>,
    #[serde(default = "one")]
    pub horizon: f64,
    #[serde(default)]
    pub domains: Vec<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictRecord {
    pub campaign: usize,
    pub passed: bool,
    pub checks: Vec<CheckView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueRecord {
    pub campaign: usize,
    pub findings: Vec<Finding>,
    pub proposal_rationale: Option<String>,
}

/// Everything agents learn during a session.
#[derive(Default)]
pub struct Blackboard {
    pub text: Option<String>,
    pub understanding: Option<simorph_svc_problem_understanding::UnderstandingResult>,
    pub spec: Option<ProblemSpec>,
    pub plan_markdown: Option<String>,
    pub assembly_markdown: Option<String>,
    pub user_model: Option<UserModel>,
    pub generation: Option<simorph_svc_simulator_factory::GenerationReport>,
    pub campaigns: Vec<Campaign>,
    pub campaign_reasons: Vec<String>,
    pub verdicts: Vec<VerdictRecord>,
    pub critiques: Vec<CritiqueRecord>,
    pub reformulations: usize,
    pub archive: Option<(usize, usize, String)>,
    pub blocked: Option<String>,
}

pub struct AgentContext {
    pub board: Blackboard,
    pub workdir: PathBuf,
    pub max_reformulations: usize,
    components: ComponentSet,
    ledger: Ledger,
    policy: Arc<Policy>,
    verdict_slot: Vec<VerdictRecord>,
}

/// Operations an agent can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    ReadCatalogue,
    RunCampaign,
    GenerateSimulator,
    Archive,
    WriteArtifact,
    RecordVerdict,
    /// Change success criteria, sealed references or permissions.
    AlterProtected,
    /// Act on the outside world (deploy, actuate, purchase…).
    ExternalAction,
}

impl Operation {
    fn permissions(self) -> Vec<Permission> {
        use Permission::*;
        match self {
            Operation::ReadCatalogue => vec![ReadWorkspace],
            Operation::RunCampaign => vec![ReadWorkspace, WriteWorkspace, SpawnProcess],
            Operation::GenerateSimulator => vec![ReadWorkspace, WriteWorkspace, SpawnProcess],
            Operation::Archive | Operation::WriteArtifact => vec![WriteWorkspace],
            Operation::RecordVerdict => vec![WriteWorkspace],
            Operation::AlterProtected => vec![SelfModify],
            Operation::ExternalAction => vec![ExternalSideEffect],
        }
    }

    /// Roles that may attempt the operation at all.
    fn roles(self) -> &'static [AgentRole] {
        use AgentRole::*;
        match self {
            Operation::ReadCatalogue => &[
                Interpreter,
                Planner,
                Modeler,
                Experimenter,
                Critic,
                Validator,
                Archivist,
            ],
            Operation::RunCampaign => &[Experimenter],
            Operation::GenerateSimulator => &[Modeler],
            Operation::Archive => &[Archivist],
            Operation::WriteArtifact => &[
                Interpreter,
                Planner,
                Modeler,
                Experimenter,
                Critic,
                Validator,
                Archivist,
            ],
            Operation::RecordVerdict => &[Validator],
            Operation::AlterProtected | Operation::ExternalAction => &[],
        }
    }
}

impl AgentContext {
    pub fn new(
        workdir: PathBuf,
        components: ComponentSet,
        ledger: Ledger,
        audit: AuditLog,
    ) -> Self {
        // The signing secret never leaves this function: agents cannot forge approvals.
        let secret: [u8; 32] = rand::random();
        Self {
            board: Blackboard::default(),
            workdir,
            max_reformulations: 1,
            components,
            ledger,
            policy: Arc::new(Policy::new(secret.to_vec(), audit)),
            verdict_slot: vec![],
        }
    }

    pub fn audit(&self) -> &AuditLog {
        self.policy.audit()
    }

    /// Check role + permissions; audited either way.
    pub fn authorize(&self, role: AgentRole, op: Operation) -> Result<()> {
        let principal = Principal::agent(role.name());
        if !op.roles().contains(&role) {
            let gated = op
                .permissions()
                .iter()
                .any(|p| matches!(p, Permission::SelfModify | Permission::ExternalSideEffect));
            if gated {
                // the policy refuses human-gated permissions to autonomous agents and audits it
                self.policy.authorize(&principal, op.permissions(), None)?;
                return Err(SimorphError::PermissionDenied(format!("{op:?} refusé")));
            }
            let reason = format!(
                "rôle `{}` non autorisé pour l'opération {op:?}",
                role.name()
            );
            self.policy
                .audit()
                .record(&principal, &perm_set(op.permissions()), false, &reason);
            return Err(SimorphError::PermissionDenied(reason));
        }
        self.policy
            .authorize(&principal, op.permissions(), None)
            .map(|_| ())
    }

    pub fn manifests(&self, role: AgentRole) -> Result<Vec<ComponentManifest>> {
        self.authorize(role, Operation::ReadCatalogue)?;
        Ok(self.components.manifests())
    }

    pub fn component_by_output(
        &self,
        role: AgentRole,
        output: &str,
    ) -> Result<Vec<ComponentManifest>> {
        Ok(self
            .manifests(role)?
            .into_iter()
            .filter(|m| m.output_schema.iter().any(|p| p.name == output))
            .collect())
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn run_campaign(
        &self,
        role: AgentRole,
        spec: ProblemSpec,
        ctl: &ExecutionControl,
    ) -> Result<Campaign> {
        self.authorize(role, Operation::RunCampaign)?;
        simorph_svc_campaign_runtime::run_campaign(
            spec,
            self.components.clone(),
            self.ledger.clone(),
            ctl,
        )
    }

    pub fn generate_simulator(
        &self,
        role: AgentRole,
        req: &simorph_svc_simulator_factory::SimulatorRequest,
        ctl: &ExecutionControl,
    ) -> Result<simorph_svc_simulator_factory::GenerationReport> {
        self.authorize(role, Operation::GenerateSimulator)?;
        let mut reg = simorph_model_registry::Registry::new();
        for m in self.components.manifests() {
            reg.register_manifest(m)?;
        }
        let factory = simorph_svc_simulator_factory::SimulatorFactory::new(
            reg,
            self.components.clone(),
            self.ledger.clone(),
        );
        factory.run(req, ctl)
    }

    pub fn archive(
        &self,
        role: AgentRole,
        campaigns: &[Campaign],
    ) -> Result<(usize, usize, String)> {
        self.authorize(role, Operation::Archive)?;
        let mut k = simorph_svc_knowledge_service::KnowledgeService::new(self.ledger.clone());
        let (mut n, mut e) = (0, 0);
        for c in campaigns {
            let s = k.ingest_campaign(c)?;
            n += s.nodes_added;
            e += s.edges_added;
        }
        let dir = self.workdir.join("knowledge");
        k.save(&dir)?;
        std::fs::write(self.workdir.join("memory.md"), k.memory_report())?;
        Ok((n, e, "knowledge".to_string()))
    }

    /// Write a file inside the session directory only.
    pub fn write_artifact(&self, role: AgentRole, name: &str, content: &str) -> Result<PathBuf> {
        self.authorize(role, Operation::WriteArtifact)?;
        if name.contains("..") || name.starts_with('/') {
            return Err(SimorphError::PermissionDenied(format!(
                "chemin hors de l'espace de session : {name}"
            )));
        }
        let p = self.workdir.join(name);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, content)?;
        Ok(p)
    }

    pub fn record_verdict(&mut self, role: AgentRole, v: VerdictRecord) -> Result<()> {
        self.authorize(role, Operation::RecordVerdict)?;
        let _ = self.ledger.record(
            ProvenanceKind::Validation,
            format!("verdict indépendant campagne {}", v.campaign),
            &v,
            &[],
        );
        self.verdict_slot.push(v);
        Ok(())
    }

    /// Verdicts recorded by the Validator (read-only for everybody else).
    pub fn verdicts(&self) -> &[VerdictRecord] {
        &self.verdict_slot
    }

    /// Always refused for autonomous agents (no human approval reachable).
    pub fn alter_success_criteria(&self, role: AgentRole, _campaign: usize) -> Result<()> {
        self.authorize(role, Operation::AlterProtected)
    }

    /// Always refused for autonomous agents.
    pub fn external_action(&self, role: AgentRole, _description: &str) -> Result<()> {
        self.authorize(role, Operation::ExternalAction)
    }
}
