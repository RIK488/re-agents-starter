//! Interdisciplinary transfer (§9).  Analogies produce hypotheses only.

use crate::graph::{EdgeKind, KnowledgeGraph, KnowledgeStatus, NodeId, NodeKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::EvidenceLevel;

/// A proposed transfer of a method from a source domain to a target domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransferHypothesis {
    pub method: NodeId,
    pub method_name: String,
    pub source_domain: String,
    pub target_domain: String,
    /// Concept in the source domain the method applies to.
    pub source_concept: NodeId,
    /// Analogous concept in the target domain.
    pub target_concept: NodeId,
    /// Product of the analogy weight and the applies-to weight.
    pub confidence: f64,
    /// Always `Hypothesis`: this crate never establishes a transfer.
    pub status: KnowledgeStatus,
    pub evidence: EvidenceLevel,
    pub statement: String,
    pub falsification_criterion: String,
}

/// Find methods applied to concepts in other domains that are analogous
/// to concepts of `target_domain`, and return them as hypotheses sorted by
/// decreasing confidence.
pub fn suggest_transfers(g: &KnowledgeGraph, target_domain: &str) -> Vec<TransferHypothesis> {
    let td = target_domain.to_lowercase();
    let mut out = vec![];
    for target_concept in g
        .by_domain(&td)
        .into_iter()
        .filter(|n| n.kind == NodeKind::Concept)
    {
        for (analogue, w_analogy) in g.analogues(target_concept.id) {
            let Some(src) = g.node(analogue) else {
                continue;
            };
            if src.kind != NodeKind::Concept || src.domains.iter().any(|d| d.to_lowercase() == td) {
                continue;
            }
            // methods applying to the analogous concept
            for e in g.in_edges(src.id, Some(EdgeKind::AppliesTo)) {
                let Some(method) = g.node(e.from) else {
                    continue;
                };
                if method.kind != NodeKind::Method
                    || method.status == KnowledgeStatus::Deprecated
                    || method.status == KnowledgeStatus::Refuted
                {
                    continue;
                }
                // skip methods already applied in the target domain
                let already = g
                    .out_edges(method.id, Some(EdgeKind::AppliesTo))
                    .iter()
                    .any(|a| {
                        g.node(a.to)
                            .map_or(false, |n| n.domains.iter().any(|d| d.to_lowercase() == td))
                    });
                if already {
                    continue;
                }
                let source_domain = src.domains.first().cloned().unwrap_or_default();
                out.push(TransferHypothesis {
                    method: method.id,
                    method_name: method.name.clone(),
                    source_domain: source_domain.clone(),
                    target_domain: target_domain.to_string(),
                    source_concept: src.id,
                    target_concept: target_concept.id,
                    confidence: (w_analogy * e.weight).clamp(0.0, 1.0),
                    status: KnowledgeStatus::Hypothesis,
                    evidence: EvidenceLevel::UnverifiedHypothesis,
                    statement: format!(
                        "La méthode « {} » ({source_domain}) pourrait s'appliquer à « {} » ({target_domain}) par analogie avec « {} ».",
                        method.name, target_concept.name, src.name
                    ),
                    falsification_criterion: format!(
                        "Appliquer « {} » à un cas de référence de « {} » et comparer à la méthode de référence du domaine ; l'hypothèse est réfutée si la performance ou la validité n'est pas au moins équivalente dans le domaine de validité déclaré.",
                        method.name, target_concept.name
                    ),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Record a transfer hypothesis in the graph as a `Hypothesis` node linked
/// to the method and target concept.  Returns the node id.
pub fn record_transfer(
    g: &mut KnowledgeGraph,
    t: &TransferHypothesis,
) -> simorph_core::Result<NodeId> {
    let id = g.add_node(
        NodeKind::Hypothesis,
        &format!("transfert: {} → {}", t.method_name, t.target_domain),
        &[&t.target_domain],
        EvidenceLevel::UnverifiedHypothesis,
        KnowledgeStatus::Hypothesis,
    );
    g.set_data(id, serde_json::to_value(t)?)?;
    g.add_edge(id, t.method, EdgeKind::DerivedFrom, t.confidence)?;
    g.add_edge(id, t.target_concept, EdgeKind::AppliesTo, t.confidence)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_suggestion_is_a_hypothesis() {
        let mut g = KnowledgeGraph::new();
        let traj = g.add_node(
            NodeKind::Concept,
            "trajectory optimisation",
            &["aerospace"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let dispatch = g.add_node(
            NodeKind::Concept,
            "energy dispatch scheduling",
            &["energy"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let flows = g.add_node(
            NodeKind::Concept,
            "logistic flow routing",
            &["logistics"],
            EvidenceLevel::Computed,
            KnowledgeStatus::Established,
        );
        let colloc = g.add_node(
            NodeKind::Method,
            "direct collocation",
            &["aerospace"],
            EvidenceLevel::EmpiricallyValidated,
            KnowledgeStatus::Established,
        );
        let milp = g.add_node(
            NodeKind::Method,
            "MILP unit commitment",
            &["energy"],
            EvidenceLevel::EmpiricallyValidated,
            KnowledgeStatus::Established,
        );
        let old = g.add_node(
            NodeKind::Method,
            "deprecated shooting",
            &["aerospace"],
            EvidenceLevel::Simulated,
            KnowledgeStatus::Deprecated,
        );
        g.add_edge(colloc, traj, EdgeKind::AppliesTo, 0.9).unwrap();
        g.add_edge(old, traj, EdgeKind::AppliesTo, 0.9).unwrap();
        g.add_edge(milp, dispatch, EdgeKind::AppliesTo, 1.0)
            .unwrap();
        g.add_edge(traj, dispatch, EdgeKind::AnalogousTo, 0.6)
            .unwrap();
        g.add_edge(flows, dispatch, EdgeKind::AnalogousTo, 0.3)
            .unwrap();

        let ts = suggest_transfers(&g, "energy");
        assert_eq!(ts.len(), 1, "{ts:?}");
        let t = &ts[0];
        assert_eq!(t.method_name, "direct collocation");
        assert_eq!(t.status, KnowledgeStatus::Hypothesis);
        assert_eq!(t.evidence, EvidenceLevel::UnverifiedHypothesis);
        assert!((t.confidence - 0.54).abs() < 1e-12);
        assert!(!t.falsification_criterion.is_empty());

        // reverse direction: aerospace can borrow MILP from energy
        let back = suggest_transfers(&g, "aerospace");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].method_name, "MILP unit commitment");

        let id = record_transfer(&mut g, t).unwrap();
        assert_eq!(g.node(id).unwrap().status, KnowledgeStatus::Hypothesis);
        assert!(g.set_status(id, KnowledgeStatus::Established).is_err());
        assert!(g
            .export_memory_markdown()
            .contains("Hypothèses ouvertes (1)"));
        // once the method is applied in the target domain it is no longer suggested
        g.add_edge(colloc, dispatch, EdgeKind::AppliesTo, 0.5)
            .unwrap();
        assert!(suggest_transfers(&g, "energy").is_empty());
    }
}
