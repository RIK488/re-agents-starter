//! The laboratory: evaluation, limits, candidates, validation, decision,
//! rollback, and gated self-improvement.

use crate::improver::Improver;
use crate::policy::{decide, AcceptancePolicy, Decision};
use crate::suite::*;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use simorph_validation::{checks, classify, CheckResult, ReferenceOrigin, ValidationTarget};
use std::sync::Arc;

/// Resolves task components by id (the registry / plugin sets).
pub trait ComponentResolver: Send + Sync {
    fn resolve(&self, id: ComponentId) -> Option<Arc<dyn Component>>;
}

pub struct FnResolver<F: Fn(ComponentId) -> Option<Arc<dyn Component>> + Send + Sync>(pub F);
impl<F: Fn(ComponentId) -> Option<Arc<dyn Component>> + Send + Sync> ComponentResolver
    for FnResolver<F>
{
    fn resolve(&self, id: ComponentId) -> Option<Arc<dyn Component>> {
        (self.0)(id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Iteration {
    pub index: usize,
    pub baseline: CandidateId,
    pub candidate: Candidate,
    pub limits: Vec<Limit>,
    pub evaluation: Evaluation,
    pub decision: Decision,
    pub provenance: Option<RecordId>,
}

/// Self-improvement proposal (§5.2) on SIMORPH's own components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SelfImprovementProposal {
    pub target: String,
    pub description: String,
    /// Configuration-level change evaluated through the normal loop.
    pub candidate: Candidate,
    /// True if the proposal requires changing SIMORPH source code.
    pub changes_code: bool,
}

pub struct Laboratory {
    pub suite: SealedSuite,
    pub policy: AcceptancePolicy,
    pub resolver: Arc<dyn ComponentResolver>,
    pub ledger: Ledger,
    current: Candidate,
    current_eval: Option<Evaluation>,
    previous: Vec<(Candidate, Evaluation)>,
    pub history: Vec<Iteration>,
    suite_record: Option<RecordId>,
}

impl Laboratory {
    pub fn new(
        suite: SealedSuite,
        policy: AcceptancePolicy,
        resolver: Arc<dyn ComponentResolver>,
        baseline: Candidate,
        ledger: Ledger,
    ) -> Result<Self> {
        suite.verify()?;
        policy.validate(&suite.suite().metrics)?;
        let suite_record = ledger
            .record(
                ProvenanceKind::Dataset,
                format!(
                    "suite d'évaluation scellée {} ({})",
                    suite.suite().name,
                    suite.hash()
                ),
                &suite,
                &[],
            )
            .ok();
        Ok(Self {
            suite,
            policy,
            resolver,
            ledger,
            current: baseline,
            current_eval: None,
            previous: vec![],
            history: vec![],
            suite_record,
        })
    }

    pub fn current(&self) -> &Candidate {
        &self.current
    }
    pub fn current_evaluation(&self) -> Option<&Evaluation> {
        self.current_eval.as_ref()
    }

    /// Evaluate a candidate on the sealed suite with independent validation.
    pub fn evaluate(&self, cand: &Candidate, ctl: &ExecutionControl) -> Result<Evaluation> {
        self.suite.verify()?;
        let suite = self.suite.suite();
        let t0 = std::time::Instant::now();
        let mut raw: IndexMap<String, IndexMap<String, Vec<f64>>> = IndexMap::new();
        let mut run_ids = vec![];
        let mut failures = vec![];
        let mut checks: Vec<CheckResult> = vec![];
        let mut granted = EvidenceLevel::Computed;
        let mut first_results: Vec<(Arc<dyn Component>, TaskRequest, TaskResult)> = vec![];
        for task in &suite.tasks {
            let comp = self.resolver.resolve(task.component).ok_or_else(|| {
                SimorphError::NotFound(format!("task component {}", task.component))
            })?;
            let entry = raw.entry(task.name.clone()).or_default();
            for (i, seed) in task.seeds.iter().enumerate() {
                ctl.check()?;
                let mut req = TaskRequest::new(task.component).seed(*seed);
                for (k, v) in &task.inputs {
                    req.inputs.insert(k.clone(), v.clone());
                }
                for (k, v) in &cand.parameters {
                    req.inputs.insert(k.clone(), v.clone());
                }
                match run_component(comp.as_ref(), &req, ctl) {
                    Ok(res) => {
                        run_ids.push(res.run_id);
                        if res.evidence == EvidenceLevel::Simulated {
                            granted = EvidenceLevel::Simulated;
                        }
                        for m in &suite.metrics {
                            // scientific outputs first, then published metrics (timings, counters)
                            if let Some(v) = res
                                .get_f64(&m.output)
                                .ok()
                                .or_else(|| res.metrics.get(&m.output).copied())
                            {
                                entry.entry(m.name.clone()).or_default().push(v);
                            } else {
                                failures.push(format!(
                                    "task `{}` seed {seed}: output `{}` missing",
                                    task.name, m.output
                                ));
                            }
                        }
                        checks.push(checks::check_invariants(
                            &comp.manifest().validation_protocol.invariants,
                            &res,
                        ));
                        if i == 0 {
                            first_results.push((comp.clone(), req.clone(), res));
                        }
                    }
                    Err(e) => failures.push(format!("task `{}` seed {seed}: {e}", task.name)),
                }
            }
        }
        // Independent validation: reproducibility of the first run of each task
        // and the component's own reference cases.
        for (comp, req, res) in &first_results {
            if let Ok(again) = run_component(comp.as_ref(), req, ctl) {
                checks.push(checks::check_reproducibility(res, &again, 1e-9, 1e-12));
            }
            let cases = &comp.manifest().validation_protocol.reference_cases;
            if !cases.is_empty() {
                checks.extend(checks::check_reference_cases(comp.as_ref(), cases, ctl));
            }
        }
        let mut metrics = IndexMap::new();
        for m in &suite.metrics {
            let values: Vec<f64> = raw
                .values()
                .filter_map(|t| t.get(&m.name))
                .flatten()
                .cloned()
                .collect();
            if !values.is_empty() {
                metrics.insert(m.name.clone(), MetricValue::of(&values));
            }
        }
        // Classification by the validator; reference origins are analytic here
        // (the suite is synthetic), so the level can never exceed Computed/Simulated.
        let pseudo = TaskResult {
            run_id: RunId::new(),
            component: ComponentId::deterministic("lab"),
            outputs: IndexMap::new(),
            metrics: IndexMap::new(),
            evidence: granted,
            request_hash: cand.content_hash(),
            warnings: vec![],
            wall_time_ms: 0,
        };
        let record = classify(
            ValidationTarget::Claim {
                statement: format!(
                    "évaluation du candidat {} sur la suite {}",
                    cand.name,
                    self.suite.hash()
                ),
            },
            &pseudo,
            checks,
            &[ReferenceOrigin::Analytic],
            None,
            Some(self.suite.hash().to_string()),
        );
        let evidence = record.verdict;
        let vid = record.id;
        let _ = self.ledger.record(
            ProvenanceKind::Validation,
            format!("validation de l'évaluation de {}", cand.name),
            &record,
            &[],
        );
        Ok(Evaluation {
            candidate: cand.id,
            suite_hash: self.suite.hash().to_string(),
            metrics,
            raw,
            run_ids,
            validation: Some(vid),
            evidence,
            failures,
            wall_time_s: t0.elapsed().as_secs_f64(),
        })
    }

    /// Identify limits: for every quality-like metric, the worst task.
    pub fn limits(&self, eval: &Evaluation) -> Vec<Limit> {
        let mut out = vec![];
        for m in &self.suite.suite().metrics {
            let Some(v) = eval.metrics.get(&m.name) else {
                continue;
            };
            let worst = eval
                .raw
                .iter()
                .filter_map(|(task, mm)| {
                    mm.get(&m.name).map(|vals| {
                        (
                            task.clone(),
                            vals.iter().sum::<f64>() / vals.len().max(1) as f64,
                        )
                    })
                })
                .min_by(|a, b| {
                    let (x, y) = if m.higher_is_better {
                        (a.1, b.1)
                    } else {
                        (-a.1, -b.1)
                    };
                    x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                });
            if let Some((task, tv)) = worst {
                out.push(Limit {
                    metric: m.name.clone(),
                    kind: m.kind,
                    value: v.mean,
                    worst_task: task.clone(),
                    description: format!(
                        "`{}` moyenne {:.4}, pire tâche `{task}` ({tv:.4})",
                        m.name, v.mean
                    ),
                });
            }
        }
        out
    }

    /// One full loop iteration with the given improver.  Returns the decision.
    pub fn iterate(
        &mut self,
        improver: &dyn Improver,
        ctl: &ExecutionControl,
    ) -> Result<Vec<Decision>> {
        if self.current_eval.is_none() {
            let e = self.evaluate(&self.current.clone(), ctl)?;
            let _ = self.ledger.record(
                ProvenanceKind::Experiment,
                format!("évaluation initiale de {}", self.current.name),
                &e,
                &self.suite_record.into_iter().collect::<Vec<_>>(),
            );
            self.current_eval = Some(e);
        }
        let base_eval = self.current_eval.clone().expect("set above");
        let limits = self.limits(&base_eval);
        let candidates = improver.propose(&self.current, &base_eval, &limits, ctl)?;
        let mut decisions = vec![];
        for cand in candidates {
            ctl.check()?;
            let hyp = self
                .ledger
                .record(
                    ProvenanceKind::Hypothesis,
                    cand.hypothesis.clone(),
                    &cand,
                    &self.suite_record.into_iter().collect::<Vec<_>>(),
                )
                .ok();
            let eval = self.evaluate(&cand, ctl)?;
            let exp = self
                .ledger
                .record(
                    ProvenanceKind::Experiment,
                    format!("évaluation de {}", cand.name),
                    &eval,
                    &hyp.into_iter().collect::<Vec<_>>(),
                )
                .ok();
            let base_for_decision = self.current_eval.clone().expect("exists");
            let decision = decide(
                &self.policy,
                &self.suite.suite().metrics,
                &base_for_decision,
                &eval,
            );
            let kind = if decision.accepted() {
                ProvenanceKind::Improvement
            } else {
                ProvenanceKind::Decision
            };
            let rec = self
                .ledger
                .record(
                    kind,
                    format!(
                        "{} : {}",
                        cand.name,
                        if decision.accepted() {
                            "accepté"
                        } else {
                            "rejeté"
                        }
                    ),
                    &decision,
                    &exp.into_iter().collect::<Vec<_>>(),
                )
                .ok();
            let accepted = decision.accepted();
            self.history.push(Iteration {
                index: self.history.len(),
                baseline: self.current.id,
                candidate: cand.clone(),
                limits: limits.clone(),
                evaluation: eval.clone(),
                decision: decision.clone(),
                provenance: rec,
            });
            if accepted {
                let prev_eval = self.current_eval.take().expect("exists");
                self.previous
                    .push((std::mem::replace(&mut self.current, cand), prev_eval));
                self.current_eval = Some(eval);
            }
            decisions.push(decision);
        }
        Ok(decisions)
    }

    /// Revert the last accepted candidate.
    pub fn rollback(&mut self) -> Result<Candidate> {
        let (prev, prev_eval) = self
            .previous
            .pop()
            .ok_or_else(|| SimorphError::InvalidInput("nothing to roll back".into()))?;
        let dropped = std::mem::replace(&mut self.current, prev);
        self.current_eval = Some(prev_eval);
        let _ = self.ledger.record(
            ProvenanceKind::Correction,
            format!(
                "retour arrière : {} retiré, {} restauré",
                dropped.name, self.current.name
            ),
            &dropped,
            &[],
        );
        Ok(dropped)
    }

    /// Evaluate a self-improvement proposal.  Configuration-level proposals
    /// go through the normal loop; code-level ones require `SelfModify`,
    /// which is never granted here.
    pub fn assess_self_improvement(
        &mut self,
        proposal: &SelfImprovementProposal,
        ctl: &ExecutionControl,
    ) -> Result<Decision> {
        if proposal.changes_code {
            let _ = self.ledger.record(
                ProvenanceKind::Decision,
                format!(
                    "auto-amélioration refusée sans approbation humaine : {}",
                    proposal.target
                ),
                proposal,
                &[],
            );
            return Err(SimorphError::PermissionDenied(format!(
                "proposal `{}` changes SIMORPH code: requires Permission::SelfModify with human approval (security policy), cannot be applied autonomously",
                proposal.target
            )));
        }
        if self.current_eval.is_none() {
            self.current_eval = Some(self.evaluate(&self.current.clone(), ctl)?);
        }
        let eval = self.evaluate(&proposal.candidate, ctl)?;
        let d = decide(
            &self.policy,
            &self.suite.suite().metrics,
            self.current_eval.as_ref().expect("set"),
            &eval,
        );
        let _ = self.ledger.record(
            ProvenanceKind::Decision,
            format!(
                "auto-amélioration {} : {}",
                proposal.target,
                if d.accepted() {
                    "acceptée (bac à sable)"
                } else {
                    "rejetée"
                }
            ),
            &d,
            &[],
        );
        if d.accepted() {
            let prev_eval = self.current_eval.take().expect("exists");
            self.previous.push((
                std::mem::replace(&mut self.current, proposal.candidate.clone()),
                prev_eval,
            ));
            self.current_eval = Some(eval);
        }
        Ok(d)
    }

    pub fn report_markdown(&self) -> String {
        let mut s = format!(
            "# Laboratoire IA — suite {} (`{}`)\n\n",
            self.suite.suite().name,
            self.suite.hash()
        );
        s.push_str(&format!("Candidat courant : **{}**\n\n", self.current.name));
        if let Some(e) = &self.current_eval {
            s.push_str("| Métrique | Moyenne | Écart-type | n |\n|---|---|---|---|\n");
            for (k, v) in &e.metrics {
                s.push_str(&format!(
                    "| {k} | {:.4} | {:.4} | {} |\n",
                    v.mean, v.std, v.n
                ));
            }
            s.push_str(&format!(
                "\nNiveau de preuve accordé : {}\n",
                e.evidence.label_fr()
            ));
        }
        s.push_str("\n## Itérations\n");
        for it in &self.history {
            s.push_str(&format!(
                "- #{} {} — {}\n",
                it.index,
                it.candidate.name,
                match &it.decision {
                    Decision::Accepted { reasons } => format!("ACCEPTÉ : {}", reasons.join(" ; ")),
                    Decision::Rejected { reasons } => format!("rejeté : {}", reasons.join(" ; ")),
                }
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::improver::{ParameterSweepImprover, TrainingImprover};
    use serde_json::json;
    use simorph_plugin_ai as ai;

    fn resolver() -> Arc<dyn ComponentResolver> {
        Arc::new(FnResolver(|id: ComponentId| {
            ai::build(id).map(|b| Arc::from(b))
        }))
    }

    #[test]
    fn training_candidate_accepted_then_rollback() {
        let eval_id = ai::classifier::ClassifierEval::new().manifest().id;
        let suite = SealedSuite::seal(EvaluationSuite {
            name: "classifier-heldout".into(),
            version: "1".into(),
            tasks: vec![EvalTask {
                name: "heldout".into(),
                component: eval_id,
                inputs: [
                    ("dataset_seed".to_string(), json!(99)),
                    ("n_samples".to_string(), json!(1000)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1, 2, 3],
            }],
            metrics: vec![
                MetricSpec {
                    name: "accuracy".into(),
                    output: "accuracy".into(),
                    kind: MetricKind::Quality,
                    higher_is_better: true,
                    protected: true,
                    regression_tolerance: 0.0,
                },
                MetricSpec {
                    name: "log_loss".into(),
                    output: "log_loss".into(),
                    kind: MetricKind::Quality,
                    higher_is_better: false,
                    protected: true,
                    regression_tolerance: 0.0,
                },
                MetricSpec {
                    name: "latency".into(),
                    output: "latency_per_sample".into(),
                    kind: MetricKind::Speed,
                    higher_is_better: false,
                    protected: false,
                    regression_tolerance: 0.0,
                },
            ],
        })
        .unwrap();
        let policy = AcceptancePolicy {
            primary_metric: "accuracy".into(),
            min_relative_improvement: 0.05,
            min_z_score: 0.0,
            min_evidence: EvidenceLevel::Computed,
        };
        let baseline = Candidate::new(
            "zero-weights",
            [("weights".to_string(), json!([0, 0, 0, 0, 0, 0]))]
                .into_iter()
                .collect(),
            "point de départ",
        );
        let mut lab =
            Laboratory::new(suite, policy, resolver(), baseline, Ledger::in_memory()).unwrap();
        let improver = TrainingImprover {
            trainer: ai::build(ai::classifier::ClassifierTrain::new().manifest().id).unwrap(),
            param: "weights".into(),
            trainer_inputs: [
                ("dataset_seed".to_string(), json!(1)),
                ("n_samples".to_string(), json!(1500)),
                ("learning_rate".to_string(), json!(0.5)),
                ("epochs".to_string(), json!(200)),
            ]
            .into_iter()
            .collect(),
        };
        let d = lab
            .iterate(&improver, &ExecutionControl::default())
            .unwrap();
        assert!(d[0].accepted(), "{:?}", d[0]);
        assert_ne!(lab.current().name, "zero-weights");
        assert!(lab.current_evaluation().unwrap().metrics["accuracy"].mean > 0.8);
        assert!(lab.current_evaluation().unwrap().validation.is_some());
        lab.ledger.verify_chain().unwrap();
        let dropped = lab.rollback().unwrap();
        assert_eq!(lab.current().name, "zero-weights");
        assert!(dropped.name.contains("train"));
        assert!(lab.report_markdown().contains("ACCEPTÉ"));
    }

    #[test]
    fn faster_but_shallower_reasoner_is_rejected_and_deeper_accepted() {
        let rid = ai::reasoner::ArithmeticReasoner::new().manifest().id;
        let suite = SealedSuite::seal(EvaluationSuite {
            name: "arithmetic-depth5".into(),
            version: "1".into(),
            tasks: vec![EvalTask {
                name: "depth5".into(),
                component: rid,
                inputs: [
                    ("task_depth".to_string(), json!(5)),
                    ("n_tasks".to_string(), json!(200)),
                    ("task_seed".to_string(), json!(11)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1],
            }],
            metrics: vec![
                MetricSpec {
                    name: "accuracy".into(),
                    output: "accuracy".into(),
                    kind: MetricKind::Quality,
                    higher_is_better: true,
                    protected: true,
                    regression_tolerance: 0.0,
                },
                MetricSpec {
                    name: "steps".into(),
                    output: "mean_steps".into(),
                    kind: MetricKind::Cost,
                    higher_is_better: false,
                    protected: false,
                    regression_tolerance: 0.0,
                },
            ],
        })
        .unwrap();
        let policy = AcceptancePolicy {
            primary_metric: "accuracy".into(),
            min_relative_improvement: 0.01,
            min_z_score: 0.0,
            min_evidence: EvidenceLevel::Computed,
        };
        let baseline = Candidate::new(
            "depth-3",
            [("max_depth".to_string(), json!(3))].into_iter().collect(),
            "base",
        );
        let mut lab =
            Laboratory::new(suite, policy, resolver(), baseline, Ledger::in_memory()).unwrap();
        let sweep = ParameterSweepImprover {
            param: "max_depth".into(),
            deltas: vec![-1.0, 3.0],
            min: 0.0,
            max: 10.0,
        };
        let d = lab.iterate(&sweep, &ExecutionControl::default()).unwrap();
        assert_eq!(d.len(), 2);
        assert!(!d[0].accepted(), "shallower must be rejected: {:?}", d[0]);
        assert!(d[1].accepted(), "deeper must be accepted: {:?}", d[1]);
        assert_eq!(lab.current().parameters["max_depth"], json!(6.0));
    }

    #[test]
    fn code_level_self_improvement_is_refused() {
        let rid = ai::reasoner::ArithmeticReasoner::new().manifest().id;
        let suite = SealedSuite::seal(EvaluationSuite {
            name: "s".into(),
            version: "1".into(),
            tasks: vec![EvalTask {
                name: "t".into(),
                component: rid,
                inputs: [
                    ("task_depth".to_string(), json!(3)),
                    ("n_tasks".to_string(), json!(50)),
                    ("task_seed".to_string(), json!(1)),
                ]
                .into_iter()
                .collect(),
                seeds: vec![1],
            }],
            metrics: vec![MetricSpec {
                name: "accuracy".into(),
                output: "accuracy".into(),
                kind: MetricKind::Quality,
                higher_is_better: true,
                protected: true,
                regression_tolerance: 0.0,
            }],
        })
        .unwrap();
        let policy = AcceptancePolicy {
            primary_metric: "accuracy".into(),
            min_relative_improvement: 0.0,
            min_z_score: 0.0,
            min_evidence: EvidenceLevel::Computed,
        };
        let baseline = Candidate::new(
            "b",
            [("max_depth".to_string(), json!(2))].into_iter().collect(),
            "base",
        );
        let mut lab = Laboratory::new(
            suite,
            policy,
            resolver(),
            baseline.clone(),
            Ledger::in_memory(),
        )
        .unwrap();
        let p = SelfImprovementProposal {
            target: "simorph-scheduler".into(),
            description: "rewrite".into(),
            candidate: baseline.clone(),
            changes_code: true,
        };
        assert!(matches!(
            lab.assess_self_improvement(&p, &ExecutionControl::default()),
            Err(SimorphError::PermissionDenied(_))
        ));
        let cfg = SelfImprovementProposal {
            target: "reasoner depth".into(),
            description: "config".into(),
            candidate: baseline.child(
                "deeper",
                [("max_depth".to_string(), json!(5))].into_iter().collect(),
                "h",
            ),
            changes_code: false,
        };
        assert!(lab
            .assess_self_improvement(&cfg, &ExecutionControl::default())
            .unwrap()
            .accepted());
    }
}
