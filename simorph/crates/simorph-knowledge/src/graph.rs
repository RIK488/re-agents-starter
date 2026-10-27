//! Knowledge graph.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{EvidenceLevel, Result, SimorphError, TypedId};
use simorph_provenance::RecordId;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeMarker;
pub type NodeId = TypedId<NodeMarker>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Concept,
    Method,
    Domain,
    Model,
    Dataset,
    Result,
    Failure,
    Decision,
    Improvement,
    Correction,
    Hypothesis,
    Experiment,
    Simulator,
    ArchitectureDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Established,
    Hypothesis,
    Refuted,
    Deprecated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Domain tags (free-form).
    #[serde(default)]
    pub domains: Vec<String>,
    pub evidence: EvidenceLevel,
    pub status: KnowledgeStatus,
    /// Arbitrary structured data (claim values, metrics, …).
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub provenance: Option<RecordId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Identifier of the validation record (e.g. a
    /// `simorph_validation::ValidationRecord` id) that justifies an
    /// `Established` hypothesis or a `MathematicallyProven` evidence level.
    #[serde(default)]
    pub validation: Option<String>,
}

/// Why a node's claimed status/evidence is not acceptable, if it is not.
fn claim_problem(
    kind: NodeKind,
    evidence: EvidenceLevel,
    status: KnowledgeStatus,
    validation: Option<&str>,
) -> Option<String> {
    let _ = kind;
    if status == KnowledgeStatus::Established && evidence == EvidenceLevel::UnverifiedHypothesis {
        return Some("cannot be established: evidence level is unverified hypothesis".into());
    }
    if evidence == EvidenceLevel::MathematicallyProven && validation.is_none() {
        return Some("claims a mathematical proof without a validation id".into());
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    AppliesTo,
    AnalogousTo,
    Generalizes,
    DerivedFrom,
    Contradicts,
    Supersedes,
    UsedIn,
    Supports,
    Refutes,
    BelongsTo,
    Corrects,
    Improves,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// Strength / confidence in [0, 1].
    pub weight: f64,
    #[serde(default)]
    pub provenance: Option<RecordId>,
    #[serde(default)]
    pub note: String,
}

fn check_weight(e: &Edge) -> Result<()> {
    if !(0.0..=1.0).contains(&e.weight) {
        return Err(SimorphError::InvalidInput(format!(
            "edge weight {} outside [0, 1]",
            e.weight
        )));
    }
    Ok(())
}

fn demote_if_unjustified(node: &mut Node) {
    if let Some(why) = claim_problem(
        node.kind,
        node.evidence,
        node.status,
        node.validation.as_deref(),
    ) {
        let claimed = serde_json::json!({
            "status": node.status, "evidence": node.evidence, "reason": why
        });
        match &mut node.data {
            serde_json::Value::Object(m) => {
                m.insert("refused_claim".into(), claimed);
            }
            serde_json::Value::Null => node.data = serde_json::json!({ "refused_claim": claimed }),
            other => {
                *other = serde_json::json!({ "value": other.clone(), "refused_claim": claimed })
            }
        }
        node.status = KnowledgeStatus::Hypothesis;
        node.evidence = EvidenceLevel::UnverifiedHypothesis;
    }
}

/// The graph.  Nodes and edges are only ever added or re-labelled; nothing
/// is deleted (deprecation is a status).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    nodes: IndexMap<NodeId, Node>,
    edges: Vec<Edge>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node.  An unacceptable claim (see [`Self::try_add_node`]) is
    /// **not** stored as claimed (audit: anything could be added as
    /// established or proven): the node is stored as an open
    /// `Hypothesis` with `UnverifiedHypothesis` evidence and the refused
    /// claim is recorded in `data.refused_claim`.  Use `try_add_node` to get
    /// an error instead.
    pub fn add_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        domains: &[&str],
        evidence: EvidenceLevel,
        status: KnowledgeStatus,
    ) -> NodeId {
        self.insert_node(kind, name, domains, evidence, status, None)
    }

    /// Add a node, refusing `Established` with unverified evidence and
    /// `MathematicallyProven` evidence without a validation id.
    pub fn try_add_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        domains: &[&str],
        evidence: EvidenceLevel,
        status: KnowledgeStatus,
    ) -> Result<NodeId> {
        if let Some(why) = claim_problem(kind, evidence, status, None) {
            return Err(SimorphError::Validation(format!("node `{name}` {why}")));
        }
        Ok(self.insert_node(kind, name, domains, evidence, status, None))
    }

    /// Add a node backed by a validation record (required for
    /// `MathematicallyProven`).
    pub fn add_validated_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        domains: &[&str],
        evidence: EvidenceLevel,
        status: KnowledgeStatus,
        validation_id: &str,
    ) -> Result<NodeId> {
        if validation_id.trim().is_empty() {
            return Err(SimorphError::Validation("empty validation id".into()));
        }
        if let Some(why) = claim_problem(kind, evidence, status, Some(validation_id)) {
            return Err(SimorphError::Validation(format!("node `{name}` {why}")));
        }
        Ok(self.insert_node(
            kind,
            name,
            domains,
            evidence,
            status,
            Some(validation_id.to_string()),
        ))
    }

    fn insert_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        domains: &[&str],
        evidence: EvidenceLevel,
        status: KnowledgeStatus,
        validation: Option<String>,
    ) -> NodeId {
        let now = Utc::now();
        let id = NodeId::new();
        let mut node = Node {
            id,
            kind,
            name: name.into(),
            description: String::new(),
            domains: domains.iter().map(|s| s.to_string()).collect(),
            evidence,
            status,
            data: serde_json::Value::Null,
            provenance: None,
            created_at: now,
            updated_at: now,
            validation,
        };
        demote_if_unjustified(&mut node);
        self.nodes.insert(id, node);
        id
    }

    /// Insert or replace a complete node.  The same rules as
    /// [`Self::add_node`] apply: an unjustified claim is demoted.
    pub fn add_node_full(&mut self, mut node: Node) -> NodeId {
        demote_if_unjustified(&mut node);
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn set_data(&mut self, id: NodeId, data: serde_json::Value) -> Result<()> {
        let n = self
            .nodes
            .get_mut(&id)
            .ok_or_else(|| SimorphError::NotFound(format!("node {id}")))?;
        // keep the record of a refused claim
        let refused = n.data.get("refused_claim").cloned();
        n.data = data;
        if let Some(r) = refused {
            match &mut n.data {
                serde_json::Value::Object(m) => {
                    m.entry("refused_claim").or_insert(r);
                }
                serde_json::Value::Null => n.data = serde_json::json!({ "refused_claim": r }),
                other => *other = serde_json::json!({ "value": other.clone(), "refused_claim": r }),
            }
        }
        n.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_provenance(&mut self, id: NodeId, rec: RecordId) -> Result<()> {
        let n = self
            .nodes
            .get_mut(&id)
            .ok_or_else(|| SimorphError::NotFound(format!("node {id}")))?;
        n.provenance = Some(rec);
        Ok(())
    }

    /// Change the status of a node.  Nothing can be established from an
    /// `UnverifiedHypothesis`, and a `Hypothesis` node (including recorded
    /// transfer hypotheses) can only become `Established` through
    /// [`Self::set_status_with_validation`] (audit: raising its evidence
    /// level was enough to establish it without any validation).
    pub fn set_status(&mut self, id: NodeId, status: KnowledgeStatus) -> Result<()> {
        self.set_status_inner(id, status, None)
    }

    /// Change the status of a node, attaching the id of the validation record
    /// that justifies it.
    pub fn set_status_with_validation(
        &mut self,
        id: NodeId,
        status: KnowledgeStatus,
        validation_id: &str,
    ) -> Result<()> {
        if validation_id.trim().is_empty() {
            return Err(SimorphError::Validation("empty validation id".into()));
        }
        self.set_status_inner(id, status, Some(validation_id))
    }

    fn set_status_inner(
        &mut self,
        id: NodeId,
        status: KnowledgeStatus,
        validation: Option<&str>,
    ) -> Result<()> {
        let n = self
            .nodes
            .get_mut(&id)
            .ok_or_else(|| SimorphError::NotFound(format!("node {id}")))?;
        let validation = validation
            .map(str::to_string)
            .or_else(|| n.validation.clone());
        if status == KnowledgeStatus::Established {
            if let Some(why) = claim_problem(n.kind, n.evidence, status, validation.as_deref()) {
                return Err(SimorphError::Validation(format!("node `{}` {why}", n.name)));
            }
            if n.kind == NodeKind::Hypothesis && validation.is_none() {
                return Err(SimorphError::Validation(format!(
                    "hypothesis `{}` cannot be established without a validation id \
                     (use set_status_with_validation)",
                    n.name
                )));
            }
        }
        n.validation = validation;
        n.status = status;
        n.updated_at = Utc::now();
        Ok(())
    }

    /// Raise the evidence level of a node (must be an allowed transition).
    pub fn set_evidence(&mut self, id: NodeId, level: EvidenceLevel) -> Result<()> {
        let n = self
            .nodes
            .get_mut(&id)
            .ok_or_else(|| SimorphError::NotFound(format!("node {id}")))?;
        if !EvidenceLevel::allowed_transition(n.evidence, level) {
            return Err(SimorphError::Validation(format!(
                "transition {:?} -> {:?} not allowed for `{}`",
                n.evidence, level, n.name
            )));
        }
        n.evidence = level;
        n.updated_at = Utc::now();
        Ok(())
    }

    pub fn add_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        weight: f64,
    ) -> Result<()> {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return Err(SimorphError::NotFound("edge endpoint".into()));
        }
        if weight.is_nan() {
            return Err(SimorphError::InvalidInput("edge weight is NaN".into()));
        }
        self.edges.push(Edge {
            from,
            to,
            kind,
            weight: weight.clamp(0.0, 1.0),
            provenance: None,
            note: String::new(),
        });
        Ok(())
    }

    pub fn add_edge_full(&mut self, edge: Edge) -> Result<()> {
        if !self.nodes.contains_key(&edge.from) || !self.nodes.contains_key(&edge.to) {
            return Err(SimorphError::NotFound("edge endpoint".into()));
        }
        check_weight(&edge)?;
        self.edges.push(edge);
        Ok(())
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }
    pub fn find_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.values().find(|n| n.name == name)
    }
    pub fn by_kind(&self, kind: NodeKind) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.kind == kind).collect()
    }
    pub fn by_status(&self, status: KnowledgeStatus) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.status == status).collect()
    }
    pub fn by_domain(&self, domain: &str) -> Vec<&Node> {
        let d = domain.to_lowercase();
        self.nodes
            .values()
            .filter(|n| n.domains.iter().any(|x| x.to_lowercase() == d))
            .collect()
    }
    /// Outgoing edges of a node, optionally filtered by kind.
    pub fn out_edges(&self, id: NodeId, kind: Option<EdgeKind>) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| e.from == id && kind.map_or(true, |k| e.kind == k))
            .collect()
    }
    pub fn in_edges(&self, id: NodeId, kind: Option<EdgeKind>) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| e.to == id && kind.map_or(true, |k| e.kind == k))
            .collect()
    }
    /// Neighbours through `AnalogousTo` in either direction.
    pub fn analogues(&self, id: NodeId) -> Vec<(NodeId, f64)> {
        self.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::AnalogousTo && (e.from == id || e.to == id))
            .map(|e| (if e.from == id { e.to } else { e.from }, e.weight))
            .collect()
    }

    /// Detect contradictions: pairs of `Result` nodes about the same concept
    /// (both have an `AppliesTo`/`DerivedFrom` edge to the same concept) whose
    /// `data.claim` values are incompatible (different booleans/strings, or
    /// numbers differing by more than the declared `data.tolerance`), plus
    /// explicit `Contradicts` edges.
    pub fn contradictions(&self) -> Vec<(NodeId, NodeId, String)> {
        let mut out = vec![];
        for e in &self.edges {
            if e.kind == EdgeKind::Contradicts {
                out.push((e.from, e.to, "explicit contradiction edge".into()));
            }
        }
        let results: Vec<&Node> = self.by_kind(NodeKind::Result);
        let subject = |n: &Node| -> Vec<NodeId> {
            self.out_edges(n.id, None)
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        EdgeKind::AppliesTo | EdgeKind::DerivedFrom | EdgeKind::BelongsTo
                    )
                })
                .map(|e| e.to)
                .collect()
        };
        for (i, a) in results.iter().enumerate() {
            for b in results.iter().skip(i + 1) {
                if a.status == KnowledgeStatus::Deprecated
                    || b.status == KnowledgeStatus::Deprecated
                {
                    continue;
                }
                let sa = subject(a);
                let sb = subject(b);
                let shared = sa.iter().any(|x| sb.contains(x));
                if !shared {
                    continue;
                }
                let (ca, cb) = (a.data.get("claim"), b.data.get("claim"));
                let incompatible = match (ca, cb) {
                    (Some(serde_json::Value::Number(x)), Some(serde_json::Value::Number(y))) => {
                        let tol = a
                            .data
                            .get("tolerance")
                            .and_then(|t| t.as_f64())
                            .unwrap_or(0.0)
                            .max(
                                b.data
                                    .get("tolerance")
                                    .and_then(|t| t.as_f64())
                                    .unwrap_or(0.0),
                            );
                        (x.as_f64().unwrap_or(0.0) - y.as_f64().unwrap_or(0.0)).abs() > tol
                    }
                    (Some(x), Some(y)) => x != y,
                    _ => false,
                };
                if incompatible {
                    out.push((
                        a.id,
                        b.id,
                        format!(
                            "`{}` claims {:?} while `{}` claims {:?} about the same concept",
                            a.name, ca, b.name, cb
                        ),
                    ));
                }
            }
        }
        out
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Load a graph, refusing edges whose endpoints do not exist and weights
    /// outside [0, 1] (audit: a hand-edited file was accepted as is).
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let g: Self = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        for (i, e) in g.edges.iter().enumerate() {
            if !g.nodes.contains_key(&e.from) || !g.nodes.contains_key(&e.to) {
                return Err(SimorphError::Validation(format!(
                    "knowledge graph edge #{i} ({:?}) references a missing node",
                    e.kind
                )));
            }
            check_weight(e).map_err(|err| SimorphError::Validation(format!("edge #{i}: {err}")))?;
        }
        for (k, n) in &g.nodes {
            if *k != n.id {
                return Err(SimorphError::Validation(format!(
                    "knowledge graph node `{}` stored under a different id",
                    n.name
                )));
            }
        }
        Ok(g)
    }

    /// Markdown summary of the scientific memory, separating the §11
    /// categories.
    pub fn export_memory_markdown(&self) -> String {
        let mut s = String::from("# Mémoire scientifique SIMORPH\n\n");
        let section = |s: &mut String, title: &str, nodes: Vec<&Node>| {
            s.push_str(&format!("## {title} ({})\n\n", nodes.len()));
            if nodes.is_empty() {
                s.push_str("_aucun_\n\n");
                return;
            }
            for n in nodes {
                s.push_str(&format!(
                    "- **{}** [{}] — {} — domaines : {}\n",
                    n.name,
                    n.evidence.label_fr(),
                    n.description,
                    n.domains.join(", ")
                ));
            }
            s.push('\n');
        };
        let established = |k: Option<NodeKind>| -> Vec<&Node> {
            self.nodes
                .values()
                .filter(|n| {
                    n.status == KnowledgeStatus::Established
                        && k.map_or(
                            !matches!(
                                n.kind,
                                NodeKind::Failure
                                    | NodeKind::Decision
                                    | NodeKind::Improvement
                                    | NodeKind::Correction
                                    | NodeKind::ArchitectureDecision
                                    | NodeKind::Hypothesis
                            ),
                            |kk| n.kind == kk,
                        )
                })
                .collect()
        };
        section(&mut s, "Connaissances établies", established(None));
        section(
            &mut s,
            "Hypothèses ouvertes",
            self.nodes
                .values()
                .filter(|n| n.status == KnowledgeStatus::Hypothesis)
                .collect(),
        );
        section(
            &mut s,
            "Hypothèses réfutées",
            self.by_status(KnowledgeStatus::Refuted),
        );
        section(&mut s, "Expériences", self.by_kind(NodeKind::Experiment));
        section(&mut s, "Modèles", self.by_kind(NodeKind::Model));
        section(&mut s, "Simulateurs", self.by_kind(NodeKind::Simulator));
        section(&mut s, "Jeux de données", self.by_kind(NodeKind::Dataset));
        section(&mut s, "Résultats", self.by_kind(NodeKind::Result));
        section(&mut s, "Échecs", self.by_kind(NodeKind::Failure));
        section(&mut s, "Corrections", self.by_kind(NodeKind::Correction));
        section(
            &mut s,
            "Décisions d'architecture",
            self.by_kind(NodeKind::ArchitectureDecision),
        );
        section(&mut s, "Décisions", self.by_kind(NodeKind::Decision));
        section(
            &mut s,
            "Améliorations mesurées",
            self.by_kind(NodeKind::Improvement),
        );
        section(
            &mut s,
            "Éléments dépréciés",
            self.by_status(KnowledgeStatus::Deprecated),
        );
        let c = self.contradictions();
        s.push_str(&format!("## Contradictions détectées ({})\n\n", c.len()));
        for (_, _, why) in c {
            s.push_str(&format!("- {why}\n"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contradiction_between_results_on_same_concept() {
        let mut g = KnowledgeGraph::new();
        let concept = g.add_node(
            NodeKind::Concept,
            "orbital decay rate",
            &["aerospace"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let r1 = g.add_node(
            NodeKind::Result,
            "decay from sim A",
            &["aerospace"],
            EvidenceLevel::Simulated,
            KnowledgeStatus::Established,
        );
        let r2 = g.add_node(
            NodeKind::Result,
            "decay from sim B",
            &["aerospace"],
            EvidenceLevel::Simulated,
            KnowledgeStatus::Established,
        );
        let r3 = g.add_node(
            NodeKind::Result,
            "unrelated",
            &["thermal"],
            EvidenceLevel::Simulated,
            KnowledgeStatus::Established,
        );
        g.set_data(r1, serde_json::json!({"claim": 1.0, "tolerance": 0.1}))
            .unwrap();
        g.set_data(r2, serde_json::json!({"claim": 2.0, "tolerance": 0.1}))
            .unwrap();
        g.set_data(r3, serde_json::json!({"claim": 5.0})).unwrap();
        g.add_edge(r1, concept, EdgeKind::AppliesTo, 1.0).unwrap();
        g.add_edge(r2, concept, EdgeKind::AppliesTo, 1.0).unwrap();
        let c = g.contradictions();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].0, c[0].1), (r1, r2));
        g.set_data(r2, serde_json::json!({"claim": 1.05, "tolerance": 0.1}))
            .unwrap();
        assert!(g.contradictions().is_empty());
        g.set_status(r2, KnowledgeStatus::Deprecated).unwrap();
        g.set_data(r2, serde_json::json!({"claim": 9.0})).unwrap();
        assert!(g.contradictions().is_empty());
        let md = g.export_memory_markdown();
        assert!(md.contains("Éléments dépréciés (1)"));
    }

    #[test]
    fn status_and_evidence_rules() {
        let mut g = KnowledgeGraph::new();
        let h = g.add_node(
            NodeKind::Hypothesis,
            "h",
            &[],
            EvidenceLevel::UnverifiedHypothesis,
            KnowledgeStatus::Hypothesis,
        );
        assert!(g.set_status(h, KnowledgeStatus::Established).is_err());
        assert!(g
            .set_evidence(h, EvidenceLevel::EmpiricallyValidated)
            .is_err());
        g.set_evidence(h, EvidenceLevel::Simulated).unwrap();
        // audit: a hypothesis needs a validation record to be established
        assert!(g.set_status(h, KnowledgeStatus::Established).is_err());
        g.set_status_with_validation(h, KnowledgeStatus::Established, "validation:test")
            .unwrap();
        assert!(g.add_edge(h, NodeId::new(), EdgeKind::UsedIn, 1.0).is_err());
    }

    #[test]
    fn persistence_and_export_separation() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = KnowledgeGraph::new();
        g.add_node(
            NodeKind::Concept,
            "established thing",
            &["x"],
            EvidenceLevel::EmpiricallyValidated,
            KnowledgeStatus::Established,
        );
        g.add_node(
            NodeKind::Hypothesis,
            "open thing",
            &["x"],
            EvidenceLevel::UnverifiedHypothesis,
            KnowledgeStatus::Hypothesis,
        );
        g.add_node(
            NodeKind::Failure,
            "failed thing",
            &["x"],
            EvidenceLevel::Simulated,
            KnowledgeStatus::Established,
        );
        g.add_node(
            NodeKind::Improvement,
            "faster solver +12%",
            &["x"],
            EvidenceLevel::EmpiricallyValidated,
            KnowledgeStatus::Established,
        );
        g.add_node(
            NodeKind::Correction,
            "fixed sign",
            &["x"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        g.add_node(
            NodeKind::Decision,
            "chose RK45",
            &["x"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let p = dir.path().join("kg.json");
        g.save(&p).unwrap();
        let back = KnowledgeGraph::load(&p).unwrap();
        assert_eq!(back.nodes().count(), 6);
        let md = back.export_memory_markdown();
        let idx = |s: &str| {
            md.find(s)
                .unwrap_or_else(|| panic!("missing `{s}` in\n{md}"))
        };
        assert!(idx("## Connaissances établies (1)") < idx("## Hypothèses ouvertes (1)"));
        assert!(idx("## Échecs (1)") < idx("## Corrections (1)"));
        assert!(idx("## Décisions (1)") < idx("## Améliorations mesurées (1)"));
        // the failure must not be listed under established knowledge
        let est_section = &md[idx("## Connaissances établies")..idx("## Hypothèses ouvertes")];
        assert!(!est_section.contains("failed thing"));
        assert!(est_section.contains("established thing"));
    }
}
