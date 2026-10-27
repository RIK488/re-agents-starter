//! Natural-language problem understanding and supervised multi-agent
//! sessions (subsystems 2 and 3), exposed as CLI / HTTP actions.
//!
//! The understanding is deterministic (lexicons, rules, unit analysis);
//! every phrase it cannot map is returned as an ambiguity or an unsupported
//! passage instead of being guessed.  Agent sessions run the seven typed
//! agents under the supervisor; the independent validator re-executes the
//! retained point before any conclusion is reported.

use crate::{default_component_set, Workspace};
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_svc_agents::{replay, Orchestrator, OrchestratorConfig, ReplayReport, SessionEnd};
use simorph_svc_problem_understanding::{understand, UnderstandingResult};
use std::path::PathBuf;

/// Understand a problem statement against the native component catalogue.
pub fn understand_text(text: &str) -> Result<UnderstandingResult> {
    if text.trim().is_empty() {
        return Err(SimorphError::InvalidInput("empty problem statement".into()));
    }
    let manifests = default_component_set()?.manifests();
    Ok(understand(text, &manifests))
}

/// Serializable outcome of a multi-agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionSummary {
    pub end: SessionEnd,
    pub messages: usize,
    pub campaigns: usize,
    /// `Some(true)` only when the independent validator accepted every campaign.
    pub validated: Option<bool>,
    pub audit_denials: usize,
    pub report_path: PathBuf,
    pub trace_path: PathBuf,
    pub report_markdown: String,
}

fn session_dir(ws: &Workspace) -> PathBuf {
    ws.root.join("agents").join(RunId::new().to_string())
}

/// Run a supervised multi-agent session from a natural-language statement.
pub fn run_agent_session(ws: &Workspace, text: &str) -> Result<AgentSessionSummary> {
    if text.trim().is_empty() {
        return Err(SimorphError::InvalidInput("empty problem statement".into()));
    }
    let cfg = OrchestratorConfig::new(session_dir(ws));
    let r = Orchestrator::new(cfg)?.run_blocking(simorph_svc_agents::MessageBody::Start {
        text: Some(text.into()),
        spec: None,
    })?;
    r.ledger.verify_chain()?;
    let validated = if r.verdicts.is_empty() {
        None
    } else {
        Some(r.verdicts.iter().all(|v| v.passed))
    };
    Ok(AgentSessionSummary {
        end: r.end,
        messages: r.trace.len(),
        campaigns: r.board.campaigns.len(),
        validated,
        audit_denials: r.audit.entries().iter().filter(|e| !e.granted).count(),
        report_path: r.report_path,
        trace_path: r.trace_path,
        report_markdown: r.report_markdown,
    })
}

/// Deterministically replay a recorded session trace.
pub fn replay_agent_session(ws: &Workspace, trace: &std::path::Path) -> Result<ReplayReport> {
    replay(trace, OrchestratorConfig::new(session_dir(ws)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_is_understood_with_explicit_ambiguities() {
        let r = understand_text(
            "Une conduite d'eau mesure 2 km de long. La vitesse d'écoulement est de 2,5 m/s, \
             la masse volumique de 1000 kg/m³, la viscosité dynamique de 1 mPa·s et la rugosité \
             de 0,05 mm. Choisir le diamètre entre 100 et 600 mm pour minimiser la perte de \
             charge, qui ne doit pas dépasser 5 bar.",
        )
        .unwrap();
        assert_eq!(r.spec.design_variables.len(), 1);
        assert_eq!(r.spec.design_variables[0].unit, "mm");
        assert!(understand_text("   ").is_err());
    }
}
