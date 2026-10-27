//! Actions exposing the model factory, the simulator factory and the
//! knowledge service.

use crate::registry::{default_component_set, default_registry};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_svc_knowledge_service::KnowledgeService;
use simorph_svc_model_factory as mf;
use simorph_svc_simulator_factory as sf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyOutput {
    pub assembly: mf::ModelAssembly,
    pub markdown: String,
}

/// Assemble the model chain for a problem (ARC-03).
pub fn assemble_model(problem: &ProblemSpec) -> Result<AssemblyOutput> {
    let registry = default_registry()?;
    let factory = mf::ModelFactory::new(&registry);
    let assembly = factory.assemble(problem)?;
    let markdown = mf::assembly_markdown(&assembly);
    Ok(AssemblyOutput { assembly, markdown })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationsRequest {
    pub name: String,
    /// Lines such as `d(x)/dt = -k*x` or `y = 2*x`.
    pub equations: Vec<String>,
    pub variables: Vec<Port>,
    pub parameters: Vec<Quantity>,
    /// Optional simulation horizon (s) to produce a trajectory sample.
    #[serde(default)]
    pub horizon: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationsOutput {
    pub descriptor: simorph_model_registry::ModelDescriptor,
    pub markdown: String,
    pub trajectory: Option<serde_json::Value>,
}

/// Build a model descriptor (and an executable model) from typed equations.
pub fn model_from_equations(
    req: &EquationsRequest,
    ctl: &ExecutionControl,
) -> Result<EquationsOutput> {
    let eqs: Vec<&str> = req.equations.iter().map(|s| s.as_str()).collect();
    let model = mf::ExpressionModel::new(&req.name, &eqs, &req.variables, &req.parameters)?;
    let descriptor = model.descriptor("0.1.0");
    let markdown = mf::descriptor_markdown(&descriptor);
    let trajectory = match req.horizon {
        Some(h) if h > 0.0 => {
            use simorph_simulation::{simulate, Solver, SolverOptions};
            let params = model.default_params();
            let tr = simulate(
                &model,
                &params,
                None,
                (0.0, h),
                &SolverOptions::new(Solver::Rk45).with_tolerances(1e-8, 1e-10),
                ctl,
            )?;
            let names: Vec<String> = tr.state_names.iter().map(|(n, _)| n.clone()).collect();
            let mut states = serde_json::Map::new();
            for (i, n) in names.iter().enumerate() {
                states.insert(
                    n.clone(),
                    serde_json::json!(tr.states.iter().map(|s| s[i]).collect::<Vec<_>>()),
                );
            }
            Some(
                serde_json::json!({"times": tr.times, "states": states, "outputs": tr.outputs, "stats": {"steps": tr.stats.steps, "rhs_evals": tr.stats.rhs_evals}}),
            )
        }
        _ => None,
    };
    Ok(EquationsOutput {
        descriptor,
        markdown,
        trajectory,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub name: String,
    pub required_outputs: Vec<String>,
    #[serde(default)]
    pub knowns: Vec<Quantity>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default = "one")]
    pub fidelity: f64,
    #[serde(default = "default_err")]
    pub target_relative_error: f64,
    #[serde(default = "default_horizon")]
    pub horizon: f64,
    /// Equations used when no registered component can produce the outputs.
    #[serde(default)]
    pub equations: Option<EquationsRequest>,
    #[serde(default)]
    pub prefer_python: bool,
}
fn one() -> f64 {
    1.0
}
fn default_err() -> f64 {
    1e-6
}
fn default_horizon() -> f64 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOutput {
    pub report: sf::GenerationReport,
    pub markdown: String,
}

/// Run the ten-step simulator generation pipeline (ARC-04).
pub fn generate_simulator(
    ws: &Workspace,
    req: &GenerateRequest,
    ctl: &ExecutionControl,
) -> Result<GenerateOutput> {
    let mut r = sf::SimulatorRequest::new(
        &req.name,
        &req.required_outputs
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    r.knowns = req.knowns.clone();
    r.domains = req.domains.clone();
    r.fidelity = req.fidelity;
    r.target_relative_error = req.target_relative_error;
    r.horizon = req.horizon;
    r.prefer_python = req.prefer_python;
    if let Some(e) = &req.equations {
        r.equations = Some(sf::EquationSpec {
            equations: e.equations.clone(),
            variables: e.variables.clone(),
            parameters: e.parameters.clone(),
            reference_cases: vec![],
            domains: req.domains.clone(),
        });
    }
    let factory = sf::SimulatorFactory::new(
        default_registry()?,
        default_component_set()?,
        ws.ledger.clone(),
    );
    let report = factory.run(&r, ctl)?;
    let markdown = report.markdown();
    let dir = ws.root.join("simulators");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{}.md", req.name.replace(['/', ' '], "_"))),
        &markdown,
    )?;
    std::fs::write(
        dir.join(format!("{}.json", req.name.replace(['/', ' '], "_"))),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(GenerateOutput { report, markdown })
}

fn knowledge_dir(ws: &Workspace) -> std::path::PathBuf {
    ws.root.join("knowledge")
}

/// Load (or create) the workspace knowledge service.
pub fn knowledge(ws: &Workspace) -> Result<KnowledgeService> {
    let dir = knowledge_dir(ws);
    if dir.exists() {
        KnowledgeService::load(&dir, ws.ledger.clone())
    } else {
        Ok(KnowledgeService::new(ws.ledger.clone()))
    }
}

/// Ingest a campaign into the workspace memory and persist it.
pub fn remember_campaign(
    ws: &Workspace,
    c: &simorph_svc_scientific_director::Campaign,
) -> Result<simorph_svc_knowledge_service::IngestSummary> {
    let mut k = knowledge(ws)?;
    let s = k.ingest_campaign(c)?;
    k.save(knowledge_dir(ws))?;
    Ok(s)
}

pub fn memory_report(ws: &Workspace) -> Result<String> {
    Ok(knowledge(ws)?.memory_report())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferView {
    pub method: String,
    pub source_domain: String,
    pub target_domain: String,
    pub confidence: f64,
    pub status: String,
    pub falsification_criterion: String,
}

pub fn transfers(ws: &Workspace, domain: &str) -> Result<Vec<TransferView>> {
    Ok(knowledge(ws)?
        .suggest_transfers(domain)
        .into_iter()
        .map(|t| TransferView {
            method: t.method_name,
            source_domain: t.source_domain,
            target_domain: t.target_domain,
            confidence: t.confidence,
            status: format!("{:?}", t.status),
            falsification_criterion: t.falsification_criterion,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_nanosat_and_build_decay_from_equations() {
        let a = assemble_model(&crate::examples::nanosat()).unwrap();
        assert!(a.markdown.contains("nanosat") || !a.assembly.descriptors.is_empty());
        let out = model_from_equations(
            &EquationsRequest {
                name: "decay".into(),
                equations: vec!["d(x)/dt = -k*x".into()],
                variables: vec![Port::scalar("x", "mol")],
                parameters: vec![
                    Quantity::new("k", 0.5, "1/s"),
                    Quantity::new("x_0", 2.0, "mol"),
                ],
                horizon: Some(4.0),
            },
            &ExecutionControl::default(),
        )
        .unwrap();
        let tr = out.trajectory.unwrap();
        let last = tr["states"]["x"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap();
        assert!((last - 2.0 * (-2.0f64).exp()).abs() < 1e-6, "{last}");
    }
}
