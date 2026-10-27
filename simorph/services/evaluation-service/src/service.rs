//! Evaluation entry point.

use crate::registry::SuiteRegistry;
use crate::scorecard::ScoreCard;
use simorph_core::*;
use simorph_provenance::Ledger;
use simorph_security::{Policy, Principal};
use simorph_svc_ai_laboratory::{
    AcceptancePolicy, Candidate, ComponentResolver, Evaluation, Laboratory, MetricKind,
};
use std::sync::Arc;

pub struct EvaluationService {
    pub registry: SuiteRegistry,
    pub ledger: Ledger,
}

impl EvaluationService {
    pub fn new(registry: SuiteRegistry, ledger: Ledger) -> Self {
        Self { registry, ledger }
    }

    /// Evaluate `candidate` on the registered suite `suite_name`.  Reading
    /// suites needs `ReadWorkspace`; the decision is audited by the policy.
    pub fn evaluate(
        &self,
        policy: &Policy,
        principal: &Principal,
        suite_name: &str,
        candidate: &Candidate,
        resolver: Arc<dyn ComponentResolver>,
        ctl: &ExecutionControl,
    ) -> Result<(Evaluation, ScoreCard)> {
        policy.authorize(principal, [Permission::ReadWorkspace], None)?;
        self.registry.verify()?;
        let suite = self
            .registry
            .get(suite_name)
            .ok_or_else(|| SimorphError::NotFound(format!("suite `{suite_name}`")))?
            .clone();
        // The acceptance policy is irrelevant for a bare evaluation; pick the
        // first quality metric so that the laboratory accepts the suite.
        let primary = suite
            .suite()
            .metrics
            .iter()
            .find(|m| {
                matches!(
                    m.kind,
                    MetricKind::Quality | MetricKind::Robustness | MetricKind::Generalisation
                )
            })
            .ok_or_else(|| {
                SimorphError::InvalidInput(format!("suite `{suite_name}` has no quality metric"))
            })?
            .name
            .clone();
        let policy_lab = AcceptancePolicy {
            primary_metric: primary,
            min_relative_improvement: 0.0,
            min_z_score: 0.0,
            min_evidence: EvidenceLevel::Computed,
        };
        let lab = Laboratory::new(
            suite.clone(),
            policy_lab,
            resolver,
            candidate.clone(),
            self.ledger.clone(),
        )?;
        let eval = lab.evaluate(candidate, ctl)?;
        let card = ScoreCard::from_evaluation(&candidate.name, suite.suite(), suite.hash(), &eval);
        let _ = self.ledger.record(
            simorph_provenance::ProvenanceKind::Experiment,
            format!("fiche d'évaluation {} / {}", candidate.name, suite_name),
            &card,
            &[],
        );
        Ok((eval, card))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard::standard_suites;
    use chrono::Duration;
    use indexmap::IndexMap;
    use serde_json::json;
    use simorph_plugin_ai as ai;
    use simorph_security::{AuditLog, HumanApproval};
    use simorph_svc_ai_laboratory::FnResolver;

    #[test]
    fn scorecard_separates_kinds_and_marks_speed_informative() {
        let policy = Policy::new(b"k".to_vec(), AuditLog::in_memory());
        let admin = Principal::admin("alice");
        let approval = HumanApproval::issue(
            &admin,
            &admin,
            [Permission::SelfModify],
            Duration::minutes(5),
            policy.signing_secret(),
        )
        .unwrap();
        let mut reg = SuiteRegistry::new();
        for s in standard_suites() {
            reg.register(&policy, &admin, Some(&approval), s).unwrap();
        }
        let svc = EvaluationService::new(reg, Ledger::in_memory());
        let resolver: Arc<dyn ComponentResolver> =
            Arc::new(FnResolver(|id: ComponentId| ai::build(id).map(Arc::from)));
        let agent = Principal::agent("lab");
        let cand = Candidate::new(
            "depth-4",
            [("max_depth".to_string(), json!(4))]
                .into_iter()
                .collect::<IndexMap<_, _>>(),
            "test",
        );
        let (eval, card) = svc
            .evaluate(
                &policy,
                &agent,
                "standard.reasoner.arithmetic",
                &cand,
                resolver,
                &ExecutionControl::default(),
            )
            .unwrap();
        assert!(card.quality.contains_key("accuracy"));
        assert!(card.cost.contains_key("mean_steps"));
        assert!(card.speed.contains_key("latency_per_task"));
        assert!(!card.quality.contains_key("mean_steps"));
        assert_eq!(eval.evidence, EvidenceLevel::Computed);
        let md = card.to_markdown();
        assert!(md.contains("informatif, jamais décisif"));
        assert!(md.contains("Qualité / raisonnement (décisif)"));
        // depth 4 reasoner solves depth-3 tasks fully, depth-7 partially
        let acc = &eval.raw["depth-3"]["accuracy"];
        assert_eq!(acc[0], 1.0);
        assert!(eval.raw["depth-7"]["accuracy"][0] < 1.0);
    }
}
