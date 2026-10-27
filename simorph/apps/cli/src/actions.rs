//! User-visible actions.  Each entry of [`ACTIONS`] is implemented by a
//! function in this module (or in `lab`/`bench`) and is exposed both by the
//! CLI and by the web API.

use crate::registry::{component_set, list_components};
use crate::workspace::Workspace;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_provenance::{ProvenanceKind, ProvenanceRecord};
use simorph_svc_campaign_runtime::CampaignRuntime;
use simorph_svc_scientific_director as dir;
use simorph_validation::{checks, classify, ReferenceOrigin, ValidationRecord, ValidationTarget};
use std::path::Path;

/// Description of an action, as shown by `simorph actions` and `/api/actions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    pub id: &'static str,
    pub space: &'static str,
    pub label_fr: &'static str,
    pub cli: &'static str,
    pub http: &'static str,
}

/// The registry of implemented actions.  Adding an entry here without an
/// implementation fails the `actions_are_implemented` test.
pub const ACTIONS: &[ActionInfo] = &[
    ActionInfo { id: "components.list", space: "Modélisation", label_fr: "Lister les composants (moteurs, simulateurs, outils)", cli: "simorph registry list [--domain D] [--capability C]", http: "GET /api/components" },
    ActionInfo { id: "components.capabilities", space: "Modélisation", label_fr: "Disponibilité des moteurs externes (SciPy, SymPy, MuJoCo, Gymnasium, PyTorch)", cli: "simorph registry capabilities", http: "GET /api/capabilities" },
    ActionInfo { id: "components.show", space: "Modélisation", label_fr: "Inspecter un manifeste de composant", cli: "simorph registry show <name>", http: "GET /api/components/{name}" },
    ActionInfo { id: "units.convert", space: "Modélisation", label_fr: "Convertir une grandeur entre unités compatibles", cli: "simorph units convert <value> <from> <to>", http: "POST /api/units/convert" },
    ActionInfo { id: "units.check", space: "Validation", label_fr: "Vérifier la cohérence dimensionnelle de deux expressions", cli: "simorph units check <a> <b>", http: "POST /api/units/check" },
    ActionInfo { id: "problem.example", space: "Projet", label_fr: "Générer un problème d'exemple (nanosat, beam, pipe, finance)", cli: "simorph problem example <name>", http: "GET /api/problems/examples/{name}" },
    ActionInfo { id: "problem.validate", space: "Projet", label_fr: "Valider la structure d'un problème", cli: "simorph problem validate <file>", http: "POST /api/problems/validate" },
    ActionInfo { id: "analysis.run", space: "Recherche", label_fr: "Analyser le problème : connu / inconnu / calculable / à modéliser / à simuler / données / preuve / vérification", cli: "simorph analyse <file>", http: "POST /api/analyse" },
    ActionInfo { id: "programme.plan", space: "Recherche", label_fr: "Formuler les hypothèses et le programme expérimental (critères figés)", cli: "simorph plan <file>", http: "POST /api/plan" },
    ActionInfo { id: "campaign.run", space: "Simulation", label_fr: "Exécuter une campagne complète (expériences, optimisation, validation, conclusions)", cli: "simorph campaign run <file> [--out DIR]", http: "POST /api/campaigns" },
    ActionInfo { id: "campaign.get", space: "Simulation", label_fr: "Consulter une campagne et son rapport", cli: "simorph campaign show <id>", http: "GET /api/campaigns/{id}" },
    ActionInfo { id: "campaign.events", space: "Simulation", label_fr: "Suivre les événements d'une campagne en direct", cli: "simorph campaign events <id>", http: "GET /api/campaigns/{id}/events" },
    ActionInfo { id: "run.execute", space: "Simulation", label_fr: "Exécuter un composant avec des entrées et le valider", cli: "simorph run <component> --input k=v ... [--fidelity F]", http: "POST /api/runs" },
    ActionInfo { id: "experiment.list", space: "Simulation", label_fr: "Lister les expériences archivées", cli: "simorph experiment list", http: "GET /api/experiments" },
    ActionInfo { id: "experiment.reproduce", space: "Validation", label_fr: "Reproduire une expérience archivée et comparer les hachages", cli: "simorph experiment reproduce <id>", http: "POST /api/experiments/{id}/reproduce" },
    ActionInfo { id: "optimisation.methods", space: "Optimisation", label_fr: "Recommander des méthodes d'optimisation selon les traits du problème", cli: "simorph optimise recommend <file>", http: "POST /api/optimisation/recommend" },
    ActionInfo { id: "uncertainty.sobol", space: "Optimisation", label_fr: "Analyse de sensibilité globale (Sobol) d'un composant", cli: "simorph uncertainty sobol <component> --input k=v --vary k=std ...", http: "POST /api/uncertainty/sobol" },
    ActionInfo { id: "lab.run", space: "IA", label_fr: "Lancer la boucle d'amélioration d'une IA sur une suite scellée", cli: "simorph lab run --suite <classifier|reasoner> [--iterations N]", http: "POST /api/lab/run" },
    ActionInfo { id: "improvement.run", space: "IA", label_fr: "Améliorer un modèle d'IA en boucle fermée (limites → données simulées validées → entraînement → suite scellée → acceptation/rejet)", cli: "simorph improve run --target pipe|pendulum [--recommended]", http: "POST /api/improvement/run" },
    ActionInfo { id: "discovery.run", space: "Modélisation", label_fr: "Découvrir une loi dans des données (régression symbolique dimensionnelle, validation hors échantillon)", cli: "simorph discover csv <file> --target y | simorph discover kepler", http: "POST /api/discovery" },
    ActionInfo { id: "lab.suites", space: "IA", label_fr: "Lister les suites d'évaluation scellées", cli: "simorph lab suites", http: "GET /api/lab/suites" },
    ActionInfo { id: "provenance.list", space: "Journal", label_fr: "Lister les enregistrements de provenance", cli: "simorph provenance list [--kind K]", http: "GET /api/provenance" },
    ActionInfo { id: "provenance.explain", space: "Journal", label_fr: "Expliquer pourquoi un résultat existe (chaîne causale)", cli: "simorph provenance explain <record-id>", http: "GET /api/provenance/{id}/explain" },
    ActionInfo { id: "provenance.verify", space: "Journal", label_fr: "Vérifier l'intégrité de la chaîne de provenance", cli: "simorph provenance verify", http: "GET /api/provenance/verify" },
    ActionInfo { id: "schemas.export", space: "Export", label_fr: "Exporter les schémas JSON des contrats", cli: "simorph schemas export <dir>", http: "GET /api/schemas" },
    ActionInfo { id: "bench.run", space: "Validation", label_fr: "Exécuter les bancs de référence (intégrateurs, Sobol, optimiseurs)", cli: "simorph bench", http: "POST /api/bench" },
    ActionInfo { id: "actions.list", space: "Projet", label_fr: "Lister les actions disponibles", cli: "simorph actions", http: "GET /api/actions" },
    ActionInfo { id: "problem.understand", space: "Projet", label_fr: "Comprendre un énoncé en langage naturel (FR/EN) : spécification, correspondances, ambiguïtés explicites", cli: "simorph understand \"<texte>\"", http: "POST /api/understand" },
    ActionInfo { id: "agents.run", space: "Recherche", label_fr: "Résoudre un énoncé par session multi-agents supervisée (validateur indépendant, trace rejouable)", cli: "simorph agents run \"<texte>\"", http: "POST /api/agents/run" },
    ActionInfo { id: "agents.replay", space: "Recherche", label_fr: "Rejouer une session multi-agents et comparer chaque message", cli: "simorph agents replay <trace.jsonl>", http: "POST /api/agents/replay" },
    ActionInfo { id: "data.inspect", space: "Données", label_fr: "Lire une source de données (CSV/TSV/JSON-lines) : schéma à unités vérifiées, empreinte, aperçu, enregistrement en provenance", cli: "simorph data inspect <fichier>", http: "POST /api/data/inspect" },
    ActionInfo { id: "fmu.inspect", space: "Simulation", label_fr: "Lire une FMU (modelDescription.xml) et en dériver le manifeste de composant", cli: "simorph fmu inspect <fichier.fmu>", http: "POST /api/fmu/inspect" },
    ActionInfo { id: "formal.check", space: "Validation", label_fr: "Prouver ou réfuter un énoncé borné (Z3, certificat re-vérifié indépendamment)", cli: "simorph formal check <request.json>", http: "POST /api/formal/check" },
    ActionInfo { id: "model.assemble", space: "Modélisation", label_fr: "Assembler la chaîne de modèles d'un problème (descripteurs, connexions, unités)", cli: "simorph model assemble <file>", http: "POST /api/models/assemble" },
    ActionInfo { id: "model.from_equations", space: "Modélisation", label_fr: "Construire et simuler un modèle à partir d'équations typées", cli: "simorph model equations <file.json>", http: "POST /api/models/from-equations" },
    ActionInfo { id: "simulator.generate", space: "Simulation", label_fr: "Générer / intégrer un simulateur (pipeline en 10 étapes)", cli: "simorph simulator generate <request.json>", http: "POST /api/simulators/generate" },
    ActionInfo { id: "knowledge.memory", space: "Recherche", label_fr: "Consulter la mémoire scientifique (connaissances, hypothèses, échecs, décisions)", cli: "simorph knowledge memory", http: "GET /api/knowledge/memory" },
    ActionInfo { id: "knowledge.transfers", space: "Recherche", label_fr: "Suggérer des transferts interdisciplinaires (hypothèses à vérifier)", cli: "simorph knowledge transfers --domain D", http: "GET /api/knowledge/transfers?domain=D" },
];

// ------------------------------------------------------------------ helpers

pub fn load_problem(path: &Path) -> Result<ProblemSpec> {
    let text = std::fs::read_to_string(path)?;
    let p: ProblemSpec = serde_json::from_str(&text)?;
    p.validate_structure()?;
    Ok(p)
}

pub fn parse_kv(pairs: &[String]) -> Result<IndexMap<String, serde_json::Value>> {
    let mut m = IndexMap::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .ok_or_else(|| SimorphError::InvalidInput(format!("expected key=value, got `{p}`")))?;
        let value = serde_json::from_str::<serde_json::Value>(v)
            .unwrap_or_else(|_| serde_json::Value::String(v.to_string()));
        m.insert(k.to_string(), value);
    }
    Ok(m)
}

// ------------------------------------------------------------------ actions

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOutput {
    pub analysis: dir::ProblemAnalysis,
    pub markdown: String,
}

pub fn analyse(problem: &ProblemSpec) -> Result<AnalysisOutput> {
    let set = component_set()?;
    let catalog = dir::StaticCatalog::new(set.manifests());
    let analysis = dir::analyse(problem, &catalog)?;
    let markdown = analysis.to_markdown();
    Ok(AnalysisOutput { analysis, markdown })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOutput {
    pub analysis: dir::ProblemAnalysis,
    pub hypotheses: Vec<Hypothesis>,
    pub programme: dir::Programme,
    pub markdown: String,
}

pub fn plan(problem: &ProblemSpec) -> Result<PlanOutput> {
    let set = component_set()?;
    let catalog = dir::StaticCatalog::new(set.manifests());
    let analysis = dir::analyse(problem, &catalog)?;
    let hypotheses = dir::formulate(problem, &analysis);
    let programme = dir::plan(problem, &analysis, &hypotheses);
    let mut markdown = analysis.to_markdown();
    markdown.push_str("\n## Hypothèses\n");
    for h in &hypotheses {
        markdown.push_str(&format!(
            "- {} — *réfutation :* {}\n",
            h.statement, h.falsification_criterion
        ));
    }
    markdown.push('\n');
    markdown.push_str(&programme.to_markdown());
    Ok(PlanOutput {
        analysis,
        hypotheses,
        programme,
        markdown,
    })
}

/// Run a full campaign inside a workspace; writes `campaigns/<id>/` with
/// `campaign.json` and `report.md`.
pub fn run_campaign(
    ws: &Workspace,
    problem: ProblemSpec,
    ctl: &ExecutionControl,
) -> Result<dir::Campaign> {
    run_campaign_with(ws, problem, component_set()?, ctl)
}

/// Run a campaign on `workers` local worker processes through the
/// distributed coordinator; returns the campaign and the cluster metrics.
pub fn run_campaign_distributed(
    ws: &Workspace,
    problem: ProblemSpec,
    workers: usize,
    slots: usize,
    ctl: &ExecutionControl,
) -> Result<(dir::Campaign, simorph_distributed::CoordinatorMetrics)> {
    let cluster = crate::cluster::LocalCluster::start(workers, slots)?;
    let set = crate::cluster::remote_component_set(
        &component_set()?,
        cluster.coordinator.clone(),
        std::time::Duration::from_secs(900),
    )?;
    let c = run_campaign_with(ws, problem, set, ctl)?;
    Ok((c, cluster.metrics()))
}

/// Run a campaign with an explicit component set (local or remote).
pub fn run_campaign_with(
    ws: &Workspace,
    problem: ProblemSpec,
    set: simorph_plugin_sdk::ComponentSet,
    ctl: &ExecutionControl,
) -> Result<dir::Campaign> {
    let catalog = dir::StaticCatalog::new(set.manifests());
    let analysis = dir::analyse(&problem, &catalog)?;
    let hypotheses = dir::formulate(&problem, &analysis);
    let programme = dir::plan(&problem, &analysis, &hypotheses);
    let prob_rec = ws.ledger.record(
        ProvenanceKind::Problem,
        problem.title.clone(),
        &problem,
        &[],
    )?;
    ws.ledger.record(
        ProvenanceKind::Decision,
        "analyse du problème",
        &analysis,
        &[prob_rec],
    )?;
    for h in &hypotheses {
        ws.ledger.record(
            ProvenanceKind::Hypothesis,
            h.statement.clone(),
            h,
            &[prob_rec],
        )?;
    }
    let mut campaign = dir::Campaign::new(problem.clone(), analysis, hypotheses, programme);
    let mut rt = CampaignRuntime::new(problem, set, ws.ledger.clone());
    rt.archive = Some(ws.archive.clone());
    let outcome = campaign.execute(&mut rt, ctl);
    let dir = ws.root.join("campaigns").join(campaign.id.to_string());
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("campaign.json"),
        serde_json::to_string_pretty(&campaign)?,
    )?;
    let mut report = campaign.report_markdown();
    if !rt.chosen_methods.is_empty() {
        report.push_str("\n## Méthodes d'optimisation retenues\n");
        for (cid, (m, why)) in &rt.chosen_methods {
            report.push_str(&format!("- composant {cid} : **{m:?}**\n"));
            for w in why {
                report.push_str(&format!("  - {w}\n"));
            }
        }
    }
    std::fs::write(dir.join("report.md"), &report)?;
    ws.ledger.record(
        ProvenanceKind::Decision,
        format!(
            "campagne {} terminée : {:?}",
            campaign.id, campaign.stop_reason
        ),
        &campaign.conclusions,
        &[prob_rec],
    )?;
    outcome?;
    // Scientific memory (§11): every campaign is ingested into the workspace knowledge graph.
    crate::factories::remember_campaign(ws, &campaign)?;
    Ok(campaign)
}

pub fn load_campaign(ws: &Workspace, id: &str) -> Result<dir::Campaign> {
    let path = ws.root.join("campaigns").join(id).join("campaign.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| SimorphError::NotFound(format!("campaign {id}")))?;
    Ok(serde_json::from_str(&text)?)
}

pub fn list_campaigns(ws: &Workspace) -> Result<Vec<String>> {
    let mut out = vec![];
    for e in std::fs::read_dir(ws.root.join("campaigns"))? {
        let e = e?;
        if e.path().join("campaign.json").exists() {
            out.push(e.file_name().to_string_lossy().to_string());
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutput {
    pub result: TaskResult,
    pub validation: ValidationRecord,
    pub manifest_name: String,
}

/// Execute one component and validate the result independently.
pub fn execute_component(
    ws: Option<&Workspace>,
    name: &str,
    inputs: IndexMap<String, serde_json::Value>,
    fidelity: f64,
    seed: Option<u64>,
    ctl: &ExecutionControl,
) -> Result<RunOutput> {
    let set = component_set()?;
    let comp = set
        .get(name)
        .ok_or_else(|| SimorphError::NotFound(format!("component `{name}`")))?;
    let mut req = TaskRequest::new(comp.manifest().id).fidelity(fidelity);
    req.seed = seed;
    req.inputs = inputs;
    let result = run_component(comp.as_ref(), &req, ctl)?;
    let again = run_component(comp.as_ref(), &req, ctl)?;
    let checks = vec![
        checks::check_invariants(&comp.manifest().validation_protocol.invariants, &result),
        checks::check_reproducibility(&result, &again, 1e-9, 1e-12),
    ];
    let validation = classify(
        ValidationTarget::Run {
            run_id: result.run_id,
        },
        &result,
        checks,
        &[ReferenceOrigin::Analytic],
        None,
        None,
    );
    if let Some(ws) = ws {
        let r = ws.ledger.record(
            ProvenanceKind::Run,
            format!("{name} run {}", result.run_id),
            &result,
            &[],
        )?;
        ws.ledger.record(
            ProvenanceKind::Validation,
            format!("validation du run {}", result.run_id),
            &validation,
            &[r],
        )?;
    }
    Ok(RunOutput {
        result,
        validation,
        manifest_name: comp.manifest().name.clone(),
    })
}

pub fn components(
    domain: Option<&str>,
    capability: Option<&str>,
) -> Result<Vec<ComponentManifest>> {
    Ok(list_components(&component_set()?, domain, capability))
}

pub fn component(name: &str) -> Result<ComponentManifest> {
    component_set()?
        .get(name)
        .map(|c| c.manifest().clone())
        .ok_or_else(|| SimorphError::NotFound(format!("component `{name}`")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceListing {
    pub total: usize,
    pub records: Vec<ProvenanceRecord>,
}

pub fn provenance_list(ws: &Workspace, kind: Option<&str>, limit: usize) -> ProvenanceListing {
    let all = ws.ledger.all();
    let total = all.len();
    let records: Vec<ProvenanceRecord> = all
        .into_iter()
        .rev()
        .filter(|r| {
            kind.map(|k| format!("{:?}", r.kind).to_lowercase() == k.to_lowercase())
                .unwrap_or(true)
        })
        .take(limit)
        .collect();
    ProvenanceListing { total, records }
}

pub fn provenance_explain(ws: &Workspace, id: &str) -> Result<String> {
    let rid = simorph_provenance::RecordId::parse(id)
        .map_err(|e| SimorphError::InvalidInput(format!("bad record id: {e}")))?;
    Ok(ws.ledger.explain(rid)?.to_markdown())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproduceOutput {
    pub report: simorph_experiment::ReproductionReport,
    pub bitwise_identical: bool,
}

pub fn reproduce_experiment(
    ws: &Workspace,
    id: &str,
    ctl: &ExecutionControl,
) -> Result<ReproduceOutput> {
    let eid = ExperimentId::parse(id)
        .map_err(|e| SimorphError::InvalidInput(format!("bad experiment id: {e}")))?;
    let record = ws.archive.get(eid)?;
    let set = component_set()?;
    let comp = set
        .get_by_id(record.plan.component_id)
        .ok_or_else(|| SimorphError::NotFound(format!("component {}", record.plan.component_id)))?;
    let report = simorph_experiment::reproduce(&record, comp.as_ref(), ctl)?;
    let bitwise_identical = report.bitwise_identical();
    ws.ledger.record(
        ProvenanceKind::Validation,
        format!("reproduction de l'expérience {id}"),
        &report,
        &[],
    )?;
    Ok(ReproduceOutput {
        report,
        bitwise_identical,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub features: simorph_optimization::ProblemFeatures,
    pub ranking: Vec<simorph_optimization::MethodRecommendation>,
}

/// Recommend optimisation methods for the first component able to produce
/// the problem's objective.
pub fn recommend_methods(problem: &ProblemSpec) -> Result<Recommendation> {
    let set = component_set()?;
    let catalog = dir::StaticCatalog::new(set.manifests());
    let analysis = dir::analyse(problem, &catalog)?;
    let cand = analysis
        .solution_candidates
        .first()
        .ok_or_else(|| SimorphError::NotFound("no component produces the objective".into()))?;
    let comp = set
        .get_by_id(cand.component)
        .ok_or_else(|| SimorphError::NotFound("component".into()))?;
    let cp = simorph_optimization::ComponentProblem::new(
        comp.as_ref(),
        problem,
        TaskRequest::new(cand.component),
        ExecutionControl::default(),
    )?;
    let features = cp.features(simorph_optimization::EvaluationCost::Cheap);
    let ranking = simorph_optimization::MethodSelector::recommend(&features);
    Ok(Recommendation { features, ranking })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SobolOutput {
    pub output: String,
    pub result: simorph_uncertainty::SobolResult,
}

/// Global sensitivity of one output of a component to uncertain inputs.
pub fn sobol(
    name: &str,
    inputs: IndexMap<String, serde_json::Value>,
    vary: IndexMap<String, f64>,
    output: &str,
    n: usize,
    ctl: &ExecutionControl,
) -> Result<SobolOutput> {
    let set = component_set()?;
    let comp = set
        .get(name)
        .ok_or_else(|| SimorphError::NotFound(format!("component `{name}`")))?;
    let specs: Vec<simorph_uncertainty::InputSpec> = vary
        .iter()
        .map(|(k, sd)| {
            let v = inputs.get(k).and_then(|x| x.as_f64()).ok_or_else(|| {
                SimorphError::InvalidInput(format!("varied input `{k}` needs a nominal value"))
            })?;
            Ok(simorph_uncertainty::InputSpec::new(
                k,
                v,
                Uncertainty::Normal { std_dev: *sd },
            ))
        })
        .collect::<Result<_>>()?;
    let names: Vec<String> = vary.keys().cloned().collect();
    let base = inputs.clone();
    let id = comp.manifest().id;
    let result = simorph_uncertainty::sobol_indices(
        &specs,
        n,
        7,
        0,
        |x| {
            let mut req = TaskRequest::new(id);
            req.inputs = base.clone();
            for (k, v) in names.iter().zip(x) {
                req.inputs.insert(k.clone(), serde_json::Value::from(*v));
            }
            let r = run_component(comp.as_ref(), &req, ctl)?;
            Ok(vec![r.get_f64(output)?])
        },
        ctl,
    )?;
    Ok(SobolOutput {
        output: output.to_string(),
        result,
    })
}

pub fn export_schemas(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = simorph_model_registry::export_schemas(dir)?;
    // Additional schemas owned by the services.
    let extra: Vec<(&str, schemars::schema::RootSchema)> = vec![
        ("campaign.schema.json", schemars::schema_for!(dir::Campaign)),
        (
            "problem-analysis.schema.json",
            schemars::schema_for!(dir::ProblemAnalysis),
        ),
        (
            "validation-record.schema.json",
            schemars::schema_for!(ValidationRecord),
        ),
        (
            "evaluation-suite.schema.json",
            schemars::schema_for!(simorph_svc_ai_laboratory::SealedSuite),
        ),
        ("claim.schema.json", schemars::schema_for!(Claim)),
    ];
    for (name, schema) in extra {
        let p = dir.join(name);
        std::fs::write(&p, serde_json::to_string_pretty(&schema)?)?;
        files.push(p);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_implemented_and_unique() {
        let mut ids: Vec<&str> = ACTIONS.iter().map(|a| a.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(n, ids.len());
        // every action id maps to an implementation branch in the CLI dispatcher
        for a in ACTIONS {
            assert!(
                crate::implemented_action_ids().contains(&a.id),
                "action {} has no implementation",
                a.id
            );
        }
    }

    #[test]
    fn examples_analyse_and_plan() {
        for name in crate::examples::names() {
            let p = crate::examples::example(name).unwrap();
            p.validate_structure().unwrap();
            let a = analyse(&p).unwrap();
            assert!(!a.analysis.assessments.is_empty(), "{name}");
            let pl = plan(&p).unwrap();
            if !p.objectives.is_empty() {
                assert!(!pl.programme.experiments.is_empty(), "{name}");
            }
        }
    }

    #[test]
    fn execute_component_validates_and_records() {
        let tmp = std::env::temp_dir().join(format!("simorph-cli-{}", uuid_like()));
        let ws = Workspace::open(&tmp).unwrap();
        let mut inputs = IndexMap::new();
        inputs.insert("arrival_rate".to_string(), serde_json::json!(1.0));
        inputs.insert("service_rate".to_string(), serde_json::json!(2.0));
        let out = execute_component(
            Some(&ws),
            "operations.queue.mm1",
            inputs,
            1.0,
            None,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(out.result.get_f64("mean_customers").unwrap(), 1.0);
        assert!(out.validation.all_passed());
        assert_eq!(ws.ledger.by_kind(ProvenanceKind::Validation).len(), 1);
        ws.ledger.verify_chain().unwrap();
        let _ = std::fs::remove_dir_all(tmp);
    }

    fn uuid_like() -> String {
        RunId::new().to_string()
    }
}
