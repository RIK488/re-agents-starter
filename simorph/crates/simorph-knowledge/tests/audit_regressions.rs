//! Regression tests for defects found by the engine audit.

use simorph_core::EvidenceLevel;
use simorph_knowledge::*;

#[test]
fn audit_transfer_hypothesis_needs_a_validation_to_be_established() {
    let mut g = KnowledgeGraph::new();
    let m = g.add_node(
        NodeKind::Method,
        "m",
        &["a"],
        EvidenceLevel::Computed,
        KnowledgeStatus::Established,
    );
    let c = g.add_node(
        NodeKind::Concept,
        "c",
        &["b"],
        EvidenceLevel::Computed,
        KnowledgeStatus::Established,
    );
    let t = TransferHypothesis {
        method: m,
        method_name: "m".into(),
        source_domain: "a".into(),
        target_domain: "b".into(),
        source_concept: c,
        target_concept: c,
        confidence: 0.5,
        status: KnowledgeStatus::Hypothesis,
        evidence: EvidenceLevel::UnverifiedHypothesis,
        statement: "m applies to c".into(),
        falsification_criterion: "x".into(),
    };
    let id = record_transfer(&mut g, &t).unwrap();
    // before: raising the evidence level was enough
    g.set_evidence(id, EvidenceLevel::Simulated).unwrap();
    assert!(g.set_status(id, KnowledgeStatus::Established).is_err());
    assert!(g
        .set_status_with_validation(id, KnowledgeStatus::Established, " ")
        .is_err());
    g.set_status_with_validation(id, KnowledgeStatus::Established, "val-123")
        .unwrap();
    let n = g.node(id).unwrap();
    assert_eq!(n.status, KnowledgeStatus::Established);
    assert_eq!(n.validation.as_deref(), Some("val-123"));
}

#[test]
fn audit_add_node_refuses_unjustified_claims() {
    let mut g = KnowledgeGraph::new();
    assert!(g
        .try_add_node(
            NodeKind::Result,
            "r",
            &[],
            EvidenceLevel::UnverifiedHypothesis,
            KnowledgeStatus::Established
        )
        .is_err());
    assert!(g
        .try_add_node(
            NodeKind::Result,
            "p",
            &[],
            EvidenceLevel::MathematicallyProven,
            KnowledgeStatus::Established
        )
        .is_err());
    // add_node keeps its signature but does not store the claim as made
    let id = g.add_node(
        NodeKind::Result,
        "p",
        &[],
        EvidenceLevel::MathematicallyProven,
        KnowledgeStatus::Established,
    );
    let n = g.node(id).unwrap();
    assert_eq!(n.status, KnowledgeStatus::Hypothesis);
    assert_eq!(n.evidence, EvidenceLevel::UnverifiedHypothesis);
    assert!(n.data.get("refused_claim").is_some());
    g.set_data(id, serde_json::json!({"claim": 1})).unwrap();
    assert!(
        g.node(id).unwrap().data.get("refused_claim").is_some(),
        "marker lost by set_data"
    );
    // with a validation id the proof is accepted
    let ok = g
        .add_validated_node(
            NodeKind::Result,
            "q",
            &[],
            EvidenceLevel::MathematicallyProven,
            KnowledgeStatus::Established,
            "cert:abc",
        )
        .unwrap();
    assert_eq!(
        g.node(ok).unwrap().evidence,
        EvidenceLevel::MathematicallyProven
    );
    // ordinary computed/simulated knowledge is unaffected
    let s = g.add_node(
        NodeKind::Result,
        "s",
        &[],
        EvidenceLevel::Simulated,
        KnowledgeStatus::Established,
    );
    assert_eq!(g.node(s).unwrap().status, KnowledgeStatus::Established);
}

#[test]
fn audit_load_rejects_dangling_edges_and_bad_weights() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("k.json");
    let mut g = KnowledgeGraph::new();
    let a = g.add_node(
        NodeKind::Concept,
        "a",
        &[],
        EvidenceLevel::Computed,
        KnowledgeStatus::Established,
    );
    let b = g.add_node(
        NodeKind::Concept,
        "b",
        &[],
        EvidenceLevel::Computed,
        KnowledgeStatus::Established,
    );
    g.add_edge(a, b, EdgeKind::AnalogousTo, 0.5).unwrap();
    g.save(&p).unwrap();
    assert!(KnowledgeGraph::load(&p).is_ok());
    let text = std::fs::read_to_string(&p).unwrap();
    // weight outside [0, 1]
    std::fs::write(&p, text.replace("\"weight\": 0.5", "\"weight\": 7.0")).unwrap();
    assert!(KnowledgeGraph::load(&p).is_err());
    // dangling edge: point `to` at a node that does not exist
    let ghost = NodeId::new().to_string();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut v2 = v.clone();
    v2["edges"][0]["to"] = serde_json::Value::String(ghost);
    std::fs::write(&p, serde_json::to_string(&v2).unwrap()).unwrap();
    assert!(KnowledgeGraph::load(&p).is_err());
    // NaN weights cannot be added
    assert!(g.add_edge(a, b, EdgeKind::Supports, f64::NAN).is_err());
}
