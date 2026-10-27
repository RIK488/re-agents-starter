//! # Knowledge service (ARC-12, REQ-MEM-1..3, REQ-TRF-1/2)
//!
//! Wraps the knowledge graph and the provenance ledger into the scientific
//! memory of SIMORPH:
//!
//! * `ingest_campaign` turns a director campaign into Hypothesis / Result /
//!   Decision / Failure nodes with the statuses and evidence levels decided
//!   by the campaign (never upgraded here);
//! * `ingest_lab` records accepted improvements (with measured deltas) and
//!   rejections from the AI laboratory;
//! * `record_failure`, `record_correction`, `record_architecture_decision`
//!   keep the §11 categories complete;
//! * `why` explains a node from its graph ancestry and the ledger trace;
//! * `memory_report` renders the memory with a §11 table;
//! * `suggest_transfers` passes through transfer *hypotheses* (§9);
//! * `save` / `load` persist the graph as JSON.

use indexmap::IndexMap;
use simorph_core::HypothesisStatus as DirHypothesisStatus;
use simorph_core::*;
use simorph_knowledge::{
    suggest_transfers, EdgeKind, KnowledgeGraph, KnowledgeStatus, Node, NodeId, NodeKind,
    TransferHypothesis,
};
use simorph_provenance::{Ledger, ProvenanceKind};
use simorph_svc_ai_laboratory::{Decision, Laboratory};
use simorph_svc_scientific_director::Campaign;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct KnowledgeService {
    pub graph: KnowledgeGraph,
    pub ledger: Ledger,
    /// Problem/campaign ids already ingested (idempotence).
    ingested: HashSet<String>,
}

/// Summary of one ingestion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IngestSummary {
    pub nodes_added: usize,
    pub edges_added: usize,
    pub skipped: bool,
}

impl KnowledgeService {
    pub fn new(ledger: Ledger) -> Self {
        Self {
            graph: KnowledgeGraph::new(),
            ledger,
            ingested: HashSet::new(),
        }
    }

    pub fn with_graph(graph: KnowledgeGraph, ledger: Ledger) -> Self {
        Self {
            graph,
            ledger,
            ingested: HashSet::new(),
        }
    }

    fn add(
        &mut self,
        kind: NodeKind,
        name: &str,
        domains: &[String],
        evidence: EvidenceLevel,
        status: KnowledgeStatus,
        description: &str,
        data: serde_json::Value,
        prov_kind: ProvenanceKind,
    ) -> NodeId {
        let d: Vec<&str> = domains.iter().map(|s| s.as_str()).collect();
        let id = self.graph.add_node(kind, name, &d, evidence, status);
        let payload = serde_json::json!({"node": id, "kind": kind, "name": name, "data": data});
        if let Ok(rec) = self
            .ledger
            .record(prov_kind, format!("mémoire : {name}"), &payload, &[])
        {
            let _ = self.graph.set_provenance(id, rec);
        }
        let _ = self.graph.set_data(
            id,
            serde_json::json!({"description": description, "payload": payload["data"]}),
        );
        if let Some(n) = self.graph.node(id).cloned() {
            let mut n2 = n;
            n2.description = description.to_string();
            self.graph.add_node_full(n2);
        }
        id
    }

    /// Ingest a campaign of the scientific director.
    pub fn ingest_campaign(&mut self, c: &Campaign) -> Result<IngestSummary> {
        let key = format!("campaign:{}", c.id);
        if self.ingested.contains(&key) {
            return Ok(IngestSummary {
                skipped: true,
                ..Default::default()
            });
        }
        let before_n = self.graph.nodes().count();
        let before_e = self.graph.edges().len();
        let domains: Vec<String> = c.analysis.disciplines.clone();
        let problem = self.add(
            NodeKind::Concept,
            &c.problem.title,
            &domains,
            EvidenceLevel::UnverifiedHypothesis,
            KnowledgeStatus::Hypothesis,
            &c.problem.description,
            serde_json::json!({"problem_id": c.problem.id}),
            ProvenanceKind::Problem,
        );
        let mut hyp_nodes: IndexMap<HypothesisId, NodeId> = IndexMap::new();
        for h in &c.hypotheses {
            let (status, evidence) = match h.status {
                DirHypothesisStatus::Supported => {
                    (KnowledgeStatus::Established, EvidenceLevel::Simulated)
                }
                DirHypothesisStatus::Refuted => (
                    KnowledgeStatus::Refuted,
                    EvidenceLevel::UnverifiedHypothesis,
                ),
                DirHypothesisStatus::Abandoned => (
                    KnowledgeStatus::Deprecated,
                    EvidenceLevel::UnverifiedHypothesis,
                ),
                DirHypothesisStatus::Open | DirHypothesisStatus::Inconclusive => (
                    KnowledgeStatus::Hypothesis,
                    EvidenceLevel::UnverifiedHypothesis,
                ),
            };
            let id = self.add(
                NodeKind::Hypothesis,
                &h.statement,
                &domains,
                evidence,
                status,
                &h.falsification_criterion,
                serde_json::json!({"hypothesis_id": h.id, "status": h.status, "notes": h.notes}),
                ProvenanceKind::Hypothesis,
            );
            self.graph.add_edge(id, problem, EdgeKind::BelongsTo, 1.0)?;
            hyp_nodes.insert(h.id, id);
        }
        let mut result_nodes: IndexMap<String, NodeId> = IndexMap::new();
        for concl in &c.conclusions {
            let status = match concl.level {
                EvidenceLevel::UnverifiedHypothesis => KnowledgeStatus::Hypothesis,
                _ => KnowledgeStatus::Established,
            };
            let id = self.add(
                NodeKind::Result,
                &format!("{} = {:.6e} [{}]", concl.variable, concl.value, concl.unit),
                &domains,
                concl.level,
                status,
                &concl.statement,
                serde_json::json!({"variable": concl.variable, "claim": concl.value, "unit": concl.unit, "tolerance": 0.05, "runs": concl.supporting_runs, "validations": concl.validations, "caveats": concl.caveats}),
                ProvenanceKind::Knowledge,
            );
            self.graph.add_edge(id, problem, EdgeKind::BelongsTo, 1.0)?;
            for h in &concl.hypotheses {
                if let Some(hn) = hyp_nodes.get(h) {
                    self.graph.add_edge(id, *hn, EdgeKind::Supports, 1.0)?;
                }
            }
            result_nodes.insert(concl.variable.clone(), id);
        }
        for ct in &c.contradictions {
            let a = self.add(
                NodeKind::Result,
                &format!("{} = {:.6e} (run {})", ct.variable, ct.value_a, ct.run_a),
                &domains,
                EvidenceLevel::Simulated,
                KnowledgeStatus::Hypothesis,
                "valeur en contradiction",
                serde_json::json!({"variable": ct.variable, "claim": ct.value_a, "run": ct.run_a}),
                ProvenanceKind::Knowledge,
            );
            let b = self.add(
                NodeKind::Result,
                &format!("{} = {:.6e} (run {})", ct.variable, ct.value_b, ct.run_b),
                &domains,
                EvidenceLevel::Simulated,
                KnowledgeStatus::Hypothesis,
                "valeur en contradiction",
                serde_json::json!({"variable": ct.variable, "claim": ct.value_b, "run": ct.run_b}),
                ProvenanceKind::Knowledge,
            );
            self.graph
                .add_edge(a, b, EdgeKind::Contradicts, ct.relative_gap.min(1.0))?;
        }
        for o in &c.outcomes {
            let name = c
                .programme
                .experiments
                .iter()
                .find(|e| e.id == o.experiment)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| o.experiment.to_string());
            let kind = if o.succeeded() {
                NodeKind::Experiment
            } else {
                NodeKind::Failure
            };
            let id = self.add(kind, &name, &domains, EvidenceLevel::Simulated, if o.succeeded() { KnowledgeStatus::Established } else { KnowledgeStatus::Refuted }, &format!("{} runs, critères {}", o.runs.len(), if o.succeeded() { "satisfaits" } else { "non satisfaits" }), serde_json::json!({"experiment": o.experiment, "criteria": o.criteria, "information_gain": o.information_gain, "runs": o.runs.len()}), if o.succeeded() { ProvenanceKind::Experiment } else { ProvenanceKind::Failure });
            self.graph.add_edge(id, problem, EdgeKind::BelongsTo, 1.0)?;
        }
        for ev in c
            .events
            .iter()
            .filter(|e| e.kind == "stopped" || e.kind == "criteria_fixed")
        {
            let id = self.add(
                NodeKind::Decision,
                &format!("{} : {}", ev.kind, ev.detail),
                &domains,
                EvidenceLevel::Computed,
                KnowledgeStatus::Established,
                &ev.detail,
                serde_json::json!({"refs": ev.refs, "sequence": ev.sequence}),
                ProvenanceKind::Decision,
            );
            self.graph.add_edge(id, problem, EdgeKind::BelongsTo, 1.0)?;
        }
        self.ingested.insert(key);
        Ok(IngestSummary {
            nodes_added: self.graph.nodes().count() - before_n,
            edges_added: self.graph.edges().len() - before_e,
            skipped: false,
        })
    }

    /// Ingest the history of an AI laboratory.
    pub fn ingest_lab(&mut self, lab: &Laboratory) -> Result<IngestSummary> {
        let before_n = self.graph.nodes().count();
        let before_e = self.graph.edges().len();
        let domains = vec!["ai".to_string()];
        let suite = self.add(
            NodeKind::Dataset,
            &format!("suite {} ({})", lab.suite.suite().name, lab.suite.hash()),
            &domains,
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
            "suite d'évaluation scellée",
            serde_json::json!({"hash": lab.suite.hash()}),
            ProvenanceKind::Dataset,
        );
        let primary = lab.policy.primary_metric.clone();
        for it in &lab.history {
            let key = format!("lab-iteration:{}", it.candidate.id);
            if self.ingested.contains(&key) {
                continue;
            }
            let delta = it.evaluation.metrics.get(&primary).map(|m| m.mean);
            match &it.decision {
                Decision::Accepted { reasons } => {
                    let id = self.add(NodeKind::Improvement, &format!("amélioration acceptée : {}", it.candidate.name), &domains, it.evaluation.evidence, KnowledgeStatus::Established, &reasons.join(" ; "), serde_json::json!({"candidate": it.candidate.id, "primary_metric": primary, "value": delta, "metrics": it.evaluation.metrics, "hypothesis": it.candidate.hypothesis}), ProvenanceKind::Improvement);
                    self.graph.add_edge(id, suite, EdgeKind::UsedIn, 1.0)?;
                }
                Decision::Rejected { reasons } => {
                    let id = self.add(NodeKind::Decision, &format!("candidat rejeté : {}", it.candidate.name), &domains, it.evaluation.evidence, KnowledgeStatus::Refuted, &reasons.join(" ; "), serde_json::json!({"candidate": it.candidate.id, "primary_metric": primary, "value": delta, "hypothesis": it.candidate.hypothesis}), ProvenanceKind::Decision);
                    self.graph.add_edge(id, suite, EdgeKind::UsedIn, 1.0)?;
                }
            }
            self.ingested.insert(key);
        }
        Ok(IngestSummary {
            nodes_added: self.graph.nodes().count() - before_n,
            edges_added: self.graph.edges().len() - before_e,
            skipped: false,
        })
    }

    pub fn record_failure(&mut self, name: &str, description: &str, domains: &[String]) -> NodeId {
        self.add(
            NodeKind::Failure,
            name,
            domains,
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
            description,
            serde_json::Value::Null,
            ProvenanceKind::Failure,
        )
    }

    /// Record a correction of an earlier node (Corrects edge).
    pub fn record_correction(
        &mut self,
        corrected: NodeId,
        name: &str,
        description: &str,
    ) -> Result<NodeId> {
        let domains = self
            .graph
            .node(corrected)
            .map(|n| n.domains.clone())
            .ok_or_else(|| SimorphError::NotFound(format!("node {corrected}")))?;
        let id = self.add(
            NodeKind::Correction,
            name,
            &domains,
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
            description,
            serde_json::json!({"corrects": corrected}),
            ProvenanceKind::Correction,
        );
        self.graph
            .add_edge(id, corrected, EdgeKind::Corrects, 1.0)?;
        self.graph
            .set_status(corrected, KnowledgeStatus::Deprecated)?;
        Ok(id)
    }

    pub fn record_architecture_decision(&mut self, title: &str, adr_text: &str) -> NodeId {
        self.add(NodeKind::ArchitectureDecision, title, &["simorph".to_string()], EvidenceLevel::Computed, KnowledgeStatus::Established, adr_text, serde_json::json!({"text_sha256": simorph_core::hashing::sha256_hex(adr_text.as_bytes())}), ProvenanceKind::ArchitectureDecision)
    }

    /// Explain a node: graph ancestry (outgoing edges, transitively) and the
    /// provenance trace of its record.
    pub fn why(&self, id: NodeId) -> Result<String> {
        let node = self
            .graph
            .node(id)
            .ok_or_else(|| SimorphError::NotFound(format!("node {id}")))?;
        let mut s = format!("# Pourquoi « {} » ?\n\n", node.name);
        s.push_str(&format!(
            "- Catégorie : {:?} — statut : {:?} — niveau de preuve : {}\n- {}\n\n",
            node.kind,
            node.status,
            node.evidence.label_fr(),
            node.description
        ));
        s.push_str("## Chaîne dans la mémoire\n\n");
        let mut seen = HashSet::new();
        let mut stack = vec![(id, 0usize)];
        while let Some((cur, depth)) = stack.pop() {
            if !seen.insert(cur) || depth > 6 {
                continue;
            }
            if let Some(n) = self.graph.node(cur) {
                s.push_str(&format!(
                    "{}- [{:?}] {} ({:?}, {})\n",
                    "  ".repeat(depth),
                    n.kind,
                    n.name,
                    n.status,
                    n.evidence.label_fr()
                ));
            }
            for e in self.graph.out_edges(cur, None) {
                s.push_str(&format!(
                    "{}  → {:?} (poids {:.2})\n",
                    "  ".repeat(depth),
                    e.kind,
                    e.weight
                ));
                stack.push((e.to, depth + 1));
            }
        }
        if let Some(rec) = node.provenance {
            s.push_str("\n## Trace de provenance\n\n");
            match self.ledger.explain(rec) {
                Ok(e) => s.push_str(&e.to_markdown()),
                Err(e) => s.push_str(&format!("_trace indisponible : {e}_\n")),
            }
        }
        Ok(s)
    }

    /// Memory report: the graph export plus a §11 table.
    pub fn memory_report(&self) -> String {
        let mut s = self.graph.export_memory_markdown();
        s.push_str("\n## Tableau §11\n\n| Catégorie | Nombre | Établis | Hypothèses | Réfutés | Dépréciés |\n|---|---|---|---|---|---|\n");
        let rows: [(&str, Vec<NodeKind>); 11] = [
            (
                "Connaissances établies",
                vec![NodeKind::Concept, NodeKind::Method],
            ),
            ("Hypothèses", vec![NodeKind::Hypothesis]),
            ("Expériences", vec![NodeKind::Experiment]),
            ("Modèles", vec![NodeKind::Model]),
            ("Simulateurs", vec![NodeKind::Simulator]),
            ("Jeux de données", vec![NodeKind::Dataset]),
            ("Résultats", vec![NodeKind::Result]),
            ("Échecs", vec![NodeKind::Failure]),
            ("Corrections", vec![NodeKind::Correction]),
            (
                "Décisions d'architecture",
                vec![NodeKind::ArchitectureDecision],
            ),
            ("Améliorations mesurées", vec![NodeKind::Improvement]),
        ];
        for (label, kinds) in rows {
            let nodes: Vec<&Node> = self
                .graph
                .nodes()
                .filter(|n| kinds.contains(&n.kind))
                .collect();
            let count = |st: KnowledgeStatus| nodes.iter().filter(|n| n.status == st).count();
            s.push_str(&format!(
                "| {label} | {} | {} | {} | {} | {} |\n",
                nodes.len(),
                count(KnowledgeStatus::Established),
                count(KnowledgeStatus::Hypothesis),
                count(KnowledgeStatus::Refuted),
                count(KnowledgeStatus::Deprecated)
            ));
        }
        let contradictions = self.graph.contradictions();
        if !contradictions.is_empty() {
            s.push_str(&format!(
                "\n**Contradictions détectées : {}**\n",
                contradictions.len()
            ));
        }
        s
    }

    /// Transfer hypotheses for a target domain (never established here).
    pub fn suggest_transfers(&self, target_domain: &str) -> Vec<TransferHypothesis> {
        suggest_transfers(&self.graph, target_domain)
    }

    pub fn save(&self, dir: impl AsRef<Path>) -> Result<PathBuf> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let p = dir.join("knowledge.json");
        self.graph.save(&p)?;
        let keys: Vec<&String> = self.ingested.iter().collect();
        std::fs::write(dir.join("ingested.json"), serde_json::to_vec(&keys)?)?;
        Ok(p)
    }

    pub fn load(dir: impl AsRef<Path>, ledger: Ledger) -> Result<Self> {
        let dir = dir.as_ref();
        let graph = KnowledgeGraph::load(dir.join("knowledge.json"))?;
        let ingested: HashSet<String> = match std::fs::read(dir.join("ingested.json")) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(_) => HashSet::new(),
        };
        Ok(Self {
            graph,
            ledger,
            ingested,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_svc_scientific_director as dir;

    struct FakeExec;
    impl dir::ExperimentExecutor for FakeExec {
        fn run(
            &mut self,
            planned: &dir::PlannedExperiment,
            _ctl: &ExecutionControl,
        ) -> Result<dir::ExperimentOutcome> {
            let mut runs = vec![];
            for i in 0..planned.n_points.max(1) {
                let mut inputs = IndexMap::new();
                inputs.insert("length".to_string(), 1.0 + 0.1 * i as f64);
                let mut outputs = IndexMap::new();
                outputs.insert(
                    "deflection".to_string(),
                    0.01 * (1.0 + 0.1 * i as f64).powi(3),
                );
                runs.push(dir::RunSummary {
                    run_id: RunId::new(),
                    inputs,
                    outputs,
                    evidence: EvidenceLevel::Computed,
                    validation: Some(ValidationId::new()),
                    valid: true,
                    error: None,
                });
            }
            Ok(dir::ExperimentOutcome {
                experiment: planned.id,
                runs,
                criteria: planned
                    .success_criteria
                    .iter()
                    .map(|c| (c.name.clone(), true))
                    .collect(),
                criteria_hash: planned.criteria_hash.clone(),
                information_gain: 1.0,
                cost: 1.0,
                wall_time_s: 0.1,
            })
        }
    }

    fn campaign() -> dir::Campaign {
        let m = ComponentManifest::in_process("beam", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["civil"])
            .with_inputs(vec![Port::scalar("length", "m"), Port::scalar("load", "N")])
            .with_outputs(vec![Port::scalar("deflection", "m")]);
        let mut p = ProblemSpec::new("Poutre", "flèche minimale");
        p.knowns.push(Quantity::new("load", 100.0, "N"));
        p.design_variables.push(DesignVariable {
            name: "length".into(),
            unit: "m".into(),
            low: 1.0,
            high: 3.0,
            integer: false,
            initial: None,
        });
        p.objectives.push(Objective {
            name: "min".into(),
            direction: Direction::Minimize,
            variable: "deflection".into(),
            unit: "m".into(),
            target: None,
            weight: 1.0,
        });
        let a = dir::analyse(&p, &dir::StaticCatalog::new(vec![m])).unwrap();
        let hs = dir::formulate(&p, &a);
        let prog = dir::plan(&p, &a, &hs);
        let mut c = dir::Campaign::new(p, a, hs, prog);
        c.execute(&mut FakeExec, &ExecutionControl::default())
            .unwrap();
        c
    }

    #[test]
    fn campaign_ingestion_maps_statuses_and_evidence() {
        let ledger = Ledger::in_memory();
        let mut svc = KnowledgeService::new(ledger.clone());
        let c = campaign();
        let s = svc.ingest_campaign(&c).unwrap();
        assert!(s.nodes_added >= 6, "{s:?}");
        assert!(!svc.graph.by_kind(NodeKind::Hypothesis).is_empty());
        assert!(
            svc.graph
                .by_kind(NodeKind::Hypothesis)
                .iter()
                .any(|n| n.status == KnowledgeStatus::Established),
            "supported hypotheses become established"
        );
        let results = svc.graph.by_kind(NodeKind::Result);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].evidence, EvidenceLevel::Computed);
        assert_eq!(svc.graph.by_kind(NodeKind::Experiment).len(), 4);
        assert!(!svc.graph.by_kind(NodeKind::Decision).is_empty());
        // idempotent
        assert!(svc.ingest_campaign(&c).unwrap().skipped);
        ledger.verify_chain().unwrap();
        let report = svc.memory_report();
        assert!(report.contains("Tableau §11") && report.contains("| Résultats | 1 |"));
    }

    #[test]
    fn why_returns_graph_chain_and_provenance_trace() {
        let mut svc = KnowledgeService::new(Ledger::in_memory());
        let c = campaign();
        svc.ingest_campaign(&c).unwrap();
        let r = svc.graph.by_kind(NodeKind::Result)[0].id;
        let md = svc.why(r).unwrap();
        assert!(md.contains("Chaîne dans la mémoire"));
        assert!(md.contains("BelongsTo"));
        assert!(md.contains("Trace de provenance"));
        assert!(md.contains("Poutre"));
    }

    #[test]
    fn transfer_suggestion_stays_hypothesis_and_corrections_deprecate() {
        let mut svc = KnowledgeService::new(Ledger::in_memory());
        let g = &mut svc.graph;
        let traj = g.add_node(
            NodeKind::Concept,
            "optimisation de trajectoire",
            &["aerospace"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let dispatch = g.add_node(
            NodeKind::Concept,
            "planification de production",
            &["energy"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let method = g.add_node(
            NodeKind::Method,
            "collocation pseudo-spectrale",
            &["aerospace"],
            EvidenceLevel::EmpiricallyValidated,
            KnowledgeStatus::Established,
        );
        g.add_edge(method, traj, EdgeKind::AppliesTo, 0.9).unwrap();
        g.add_edge(traj, dispatch, EdgeKind::AnalogousTo, 0.7)
            .unwrap();
        let t = svc.suggest_transfers("energy");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].status, KnowledgeStatus::Hypothesis);
        assert_eq!(t[0].evidence, EvidenceLevel::UnverifiedHypothesis);
        assert!(!t[0].falsification_criterion.is_empty());
        let f = svc.record_failure(
            "divergence du solveur",
            "pas macro trop grand",
            &["energy".into()],
        );
        let corr = svc
            .record_correction(f, "pas macro adaptatif", "doublement de pas activé")
            .unwrap();
        assert_eq!(
            svc.graph.node(f).unwrap().status,
            KnowledgeStatus::Deprecated
        );
        assert!(svc.graph.out_edges(corr, Some(EdgeKind::Corrects)).len() == 1);
        let adr = svc.record_architecture_decision("ADR-0001", "Tokio retenu");
        assert_eq!(
            svc.graph.node(adr).unwrap().kind,
            NodeKind::ArchitectureDecision
        );
    }

    #[test]
    fn persistence_round_trip() {
        let mut svc = KnowledgeService::new(Ledger::in_memory());
        let c = campaign();
        svc.ingest_campaign(&c).unwrap();
        let dir = tempfile::tempdir().unwrap();
        svc.save(dir.path()).unwrap();
        let back = KnowledgeService::load(dir.path(), Ledger::in_memory()).unwrap();
        assert_eq!(back.graph.nodes().count(), svc.graph.nodes().count());
        assert_eq!(back.graph.edges().len(), svc.graph.edges().len());
        let mut back = back;
        assert!(
            back.ingest_campaign(&c).unwrap().skipped,
            "ingestion keys persist"
        );
    }
}
