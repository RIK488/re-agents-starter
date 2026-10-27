//! Closed-loop AI improvement action (subsystem 20).

use crate::workspace::Workspace;
use simorph_core::*;
use simorph_svc_improvement_loop as il;

/// Run the closed improvement loop on a neural surrogate of a SIMORPH
/// simulator (`pipe` or `pendulum`); `recommended` uses the adaptive
/// sampling strategy (3× more simulations).  Writes `improvement/<target>-<time>/`.
pub fn run_improvement(
    ws: &Workspace,
    target: &str,
    recommended: bool,
    ctl: &ExecutionControl,
) -> Result<il::ImprovementReport> {
    let kind = match target {
        "pipe" | "pipe_flow" | "hydraulics" => il::TargetKind::PipeFlow,
        "pendulum" => il::TargetKind::PendulumPeriod,
        other => {
            return Err(SimorphError::InvalidInput(format!(
                "unknown target `{other}` (pipe | pendulum)"
            )))
        }
    };
    let cfg = if recommended {
        il::ImprovementConfig::recommended(kind)
    } else {
        il::ImprovementConfig::quick(kind)
    };
    let report = il::run_improvement_campaign_with(cfg, ws.ledger.clone(), ctl)?;
    let dir = ws.root.join("improvement").join(format!(
        "{target}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S")
    ));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("report.md"), report.to_markdown())?;
    std::fs::write(dir.join("report.json"), report.to_json())?;
    Ok(report)
}
