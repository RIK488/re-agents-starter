//! Campaign state machine: execution, revision, contradiction detection,
//! next-experiment selection, stopping rules and traceable conclusions.

use crate::analysis::ProblemAnalysis;
use crate::hypotheses::{revise, HypothesisEvidence};
use crate::programme::{PlannedExperiment, Programme};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use std::collections::BTreeMap;

/// Summary of one run as seen by the director (produced by the executor
/// after independent validation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RunSummary {
    pub run_id: RunId,
    pub inputs: indexmap::IndexMap<String, f64>,
    pub outputs: indexmap::IndexMap<String, f64>,
    /// Evidence level *granted by the validator* (never the component's claim).
    pub evidence: EvidenceLevel,
    pub validation: Option<ValidationId>,
    pub valid: bool,
    pub error: Option<String>,
}

/// Outcome of an executed experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentOutcome {
    pub experiment: ExperimentId,
    pub runs: Vec<RunSummary>,
    /// (criterion name, passed)
    pub criteria: Vec<(String, bool)>,
    /// Hash of the criteria actually evaluated; must equal the planned hash.
    pub criteria_hash: String,
    /// Information gain estimate (e.g. variance reduction on objectives), ≥ 0.
    pub information_gain: f64,
    pub cost: f64,
    pub wall_time_s: f64,
}

impl ExperimentOutcome {
    pub fn succeeded(&self) -> bool {
        self.criteria.iter().all(|(_, p)| *p)
    }
}

/// Executes a planned experiment (orchestrator + validation live behind it).
pub trait ExperimentExecutor {
    fn run(
        &mut self,
        planned: &PlannedExperiment,
        ctl: &ExecutionControl,
    ) -> Result<ExperimentOutcome>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    ProgrammeCompleted,
    BudgetRunsExhausted,
    BudgetTimeExhausted,
    BudgetCostExhausted,
    NoInformationGain,
    UnresolvedContradictions,
    Cancelled,
    PrerequisiteFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contradiction {
    pub variable: String,
    pub run_a: RunId,
    pub run_b: RunId,
    pub value_a: f64,
    pub value_b: f64,
    pub relative_gap: f64,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Conclusion {
    pub statement: String,
    pub variable: String,
    pub value: f64,
    pub unit: String,
    pub level: EvidenceLevel,
    pub supporting_runs: Vec<RunId>,
    pub validations: Vec<ValidationId>,
    pub hypotheses: Vec<HypothesisId>,
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CampaignEvent {
    pub sequence: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub detail: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoppingRules {
    /// Stop after this many consecutive experiments with information gain below `min_information_gain`.
    pub max_unproductive: usize,
    pub min_information_gain: f64,
    /// Stop when unresolved contradictions reach this count.
    pub max_unresolved_contradictions: usize,
    /// Relative gap above which two validated values of the same variable contradict.
    pub contradiction_tolerance: f64,
}

impl Default for StoppingRules {
    fn default() -> Self {
        Self {
            max_unproductive: 3,
            min_information_gain: 1e-3,
            max_unresolved_contradictions: 3,
            contradiction_tolerance: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Campaign {
    pub id: CampaignId,
    pub problem: ProblemSpec,
    pub analysis: ProblemAnalysis,
    pub hypotheses: Vec<Hypothesis>,
    pub programme: Programme,
    pub rules: StoppingRules,
    pub outcomes: Vec<ExperimentOutcome>,
    pub contradictions: Vec<Contradiction>,
    pub conclusions: Vec<Conclusion>,
    pub events: Vec<CampaignEvent>,
    pub stop_reason: Option<StopReason>,
    pub runs_used: u64,
    pub cost_used: f64,
    pub time_used_s: f64,
}

impl Campaign {
    pub fn new(
        problem: ProblemSpec,
        analysis: ProblemAnalysis,
        hypotheses: Vec<Hypothesis>,
        programme: Programme,
    ) -> Self {
        let mut c = Self {
            id: CampaignId::new(),
            problem,
            analysis,
            hypotheses,
            programme,
            rules: StoppingRules::default(),
            outcomes: vec![],
            contradictions: vec![],
            conclusions: vec![],
            events: vec![],
            stop_reason: None,
            runs_used: 0,
            cost_used: 0.0,
            time_used_s: 0.0,
        };
        c.log(
            "campaign_created",
            "analyse, hypothèses et programme établis ; critères de réussite figés",
            vec![],
        );
        for e in &c.programme.experiments.clone() {
            c.log(
                "criteria_fixed",
                &format!("{} : {}", e.name, e.criteria_hash),
                vec![e.id.to_string()],
            );
        }
        c
    }

    fn log(&mut self, kind: &str, detail: &str, refs: Vec<String>) {
        self.events.push(CampaignEvent {
            sequence: self.events.len() as u64,
            timestamp: chrono::Utc::now(),
            kind: kind.into(),
            detail: detail.into(),
            refs,
        });
    }

    fn done_ids(&self) -> Vec<ExperimentId> {
        self.outcomes.iter().map(|o| o.experiment).collect()
    }

    /// Select the next experiment: prerequisites satisfied (and succeeded),
    /// highest utility; ties broken by declared order.
    pub fn next_experiment(&self) -> Option<PlannedExperiment> {
        let done = self.done_ids();
        let failed: Vec<ExperimentId> = self
            .outcomes
            .iter()
            .filter(|o| !o.succeeded())
            .map(|o| o.experiment)
            .collect();
        let w = self.programme.weights;
        let mut ready: Vec<&PlannedExperiment> = self
            .programme
            .experiments
            .iter()
            .filter(|e| !done.contains(&e.id))
            .filter(|e| {
                e.prerequisites
                    .iter()
                    .all(|p| done.contains(p) && !failed.contains(p))
            })
            .collect();
        ready.sort_by(|a, b| {
            b.utility(&w)
                .partial_cmp(&a.utility(&w))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ready.first().map(|e| (*e).clone())
    }

    fn pending_blocked_by_failure(&self) -> bool {
        let done = self.done_ids();
        let failed: Vec<ExperimentId> = self
            .outcomes
            .iter()
            .filter(|o| !o.succeeded())
            .map(|o| o.experiment)
            .collect();
        self.programme
            .experiments
            .iter()
            .filter(|e| !done.contains(&e.id))
            .any(|e| e.prerequisites.iter().any(|p| failed.contains(p)))
    }

    fn check_stop(&mut self) -> Option<StopReason> {
        let b = &self.problem.budget;
        if self.runs_used >= b.max_runs {
            return Some(StopReason::BudgetRunsExhausted);
        }
        if self.time_used_s >= b.max_wall_time_s {
            return Some(StopReason::BudgetTimeExhausted);
        }
        if self.cost_used >= b.max_cost {
            return Some(StopReason::BudgetCostExhausted);
        }
        let unresolved = self.contradictions.iter().filter(|c| !c.resolved).count();
        if unresolved >= self.rules.max_unresolved_contradictions {
            return Some(StopReason::UnresolvedContradictions);
        }
        let n = self.rules.max_unproductive;
        if self.outcomes.len() >= n
            && self
                .outcomes
                .iter()
                .rev()
                .take(n)
                .all(|o| o.information_gain < self.rules.min_information_gain)
        {
            return Some(StopReason::NoInformationGain);
        }
        if self.next_experiment().is_none() {
            return Some(if self.pending_blocked_by_failure() {
                StopReason::PrerequisiteFailed
            } else {
                StopReason::ProgrammeCompleted
            });
        }
        None
    }

    fn ingest(&mut self, planned: &PlannedExperiment, outcome: ExperimentOutcome) -> Result<()> {
        if outcome.criteria_hash != planned.criteria_hash {
            self.log(
                "criteria_tampering",
                &format!(
                    "{} : hash des critères évalués différent du hash planifié",
                    planned.name
                ),
                vec![planned.id.to_string()],
            );
            return Err(SimorphError::Protected(format!(
                "success criteria of `{}` were altered between planning and evaluation",
                planned.name
            )));
        }
        self.runs_used += outcome.runs.len() as u64;
        self.cost_used += outcome.cost;
        self.time_used_s += outcome.wall_time_s;
        let succeeded = outcome.succeeded();
        // hypothesis revision from validated runs only
        let valid = outcome
            .runs
            .iter()
            .filter(|r| r.valid && r.validation.is_some())
            .count() as u32;
        let invalid = outcome.runs.iter().filter(|r| !r.valid).count() as u32;
        for hid in &planned.hypotheses {
            let hid = *hid;
            if let Some(h) = self.hypotheses.iter_mut().find(|h| h.id == hid) {
                let ev = HypothesisEvidence {
                    hypothesis: hid,
                    consistent: valid,
                    contradicting: invalid + if succeeded { 0 } else { 1 },
                    notes: vec![format!(
                        "expérience {} : {}",
                        planned.name,
                        if succeeded {
                            "critères satisfaits"
                        } else {
                            "critères non satisfaits"
                        }
                    )],
                };
                revise(h, &ev);
            }
        }
        // contradiction detection across validated runs with identical inputs
        let prior: Vec<RunSummary> = self
            .outcomes
            .iter()
            .flat_map(|o| o.runs.iter().cloned())
            .filter(|r| r.valid)
            .collect();
        for r in outcome.runs.iter().filter(|r| r.valid) {
            for p in prior.iter().filter(|p| p.inputs == r.inputs) {
                for (k, v) in &r.outputs {
                    if let Some(pv) = p.outputs.get(k) {
                        let gap = (v - pv).abs() / v.abs().max(pv.abs()).max(1e-300);
                        if gap > self.rules.contradiction_tolerance {
                            self.contradictions.push(Contradiction {
                                variable: k.clone(),
                                run_a: p.run_id,
                                run_b: r.run_id,
                                value_a: *pv,
                                value_b: *v,
                                relative_gap: gap,
                                resolved: false,
                            });
                        }
                    }
                }
            }
        }
        self.log(
            "experiment_completed",
            &format!(
                "{} : {} runs, critères {}, gain informatif {:.3e}",
                planned.name,
                outcome.runs.len(),
                if succeeded { "OK" } else { "ÉCHEC" },
                outcome.information_gain
            ),
            outcome.runs.iter().map(|r| r.run_id.to_string()).collect(),
        );
        self.outcomes.push(outcome);
        Ok(())
    }

    /// Run the campaign to completion or stop.
    pub fn execute(
        &mut self,
        executor: &mut dyn ExperimentExecutor,
        ctl: &ExecutionControl,
    ) -> Result<StopReason> {
        loop {
            if ctl.check().is_err() {
                self.stop_reason = Some(StopReason::Cancelled);
                self.log("stopped", "annulation demandée", vec![]);
                break;
            }
            if let Some(r) = self.check_stop() {
                self.stop_reason = Some(r);
                self.log("stopped", &format!("{r:?}"), vec![]);
                break;
            }
            let planned = self
                .next_experiment()
                .expect("check_stop guarantees an experiment");
            self.log(
                "experiment_started",
                &planned.name,
                vec![planned.id.to_string()],
            );
            match executor.run(&planned, ctl) {
                Ok(outcome) => self.ingest(&planned, outcome)?,
                Err(e) => {
                    self.log(
                        "experiment_error",
                        &format!("{} : {e}", planned.name),
                        vec![planned.id.to_string()],
                    );
                    self.outcomes.push(ExperimentOutcome {
                        experiment: planned.id,
                        runs: vec![],
                        criteria: vec![("execution".into(), false)],
                        criteria_hash: planned.criteria_hash.clone(),
                        information_gain: 0.0,
                        cost: 0.0,
                        wall_time_s: 0.0,
                    });
                }
            }
        }
        self.conclude();
        Ok(self.stop_reason.unwrap_or(StopReason::ProgrammeCompleted))
    }

    /// Derive conclusions for every objective and unknown from validated runs.
    pub fn conclude(&mut self) {
        let mut targets: Vec<(String, String, Option<Direction>)> = self
            .problem
            .objectives
            .iter()
            .map(|o| (o.variable.clone(), o.unit.clone(), Some(o.direction)))
            .collect();
        for u in &self.problem.unknowns {
            if !targets.iter().any(|t| t.0 == u.name) {
                targets.push((u.name.clone(), u.unit.clone(), None));
            }
        }
        let runs: Vec<RunSummary> = self
            .outcomes
            .iter()
            .flat_map(|o| o.runs.iter().cloned())
            .filter(|r| r.valid)
            .collect();
        let mut conclusions = vec![];
        for (var, unit, dir) in targets {
            let mut cands: Vec<&RunSummary> = runs
                .iter()
                .filter(|r| r.outputs.contains_key(&var))
                .collect();
            if cands.is_empty() {
                conclusions.push(Conclusion {
                    statement: format!(
                        "Aucun run validé ne fournit `{var}` : la grandeur reste une inconnue."
                    ),
                    variable: var.clone(),
                    value: f64::NAN,
                    unit,
                    level: EvidenceLevel::UnverifiedHypothesis,
                    supporting_runs: vec![],
                    validations: vec![],
                    hypotheses: vec![],
                    caveats: vec!["données ou modèle supplémentaires requis".into()],
                });
                continue;
            }
            match dir {
                Some(Direction::Minimize) => {
                    cands.sort_by(|a, b| a.outputs[&var].partial_cmp(&b.outputs[&var]).unwrap())
                }
                Some(Direction::Maximize) => {
                    cands.sort_by(|a, b| b.outputs[&var].partial_cmp(&a.outputs[&var]).unwrap())
                }
                Some(Direction::Target) => {
                    let t = self
                        .problem
                        .objectives
                        .iter()
                        .find(|o| o.variable == var)
                        .and_then(|o| o.target)
                        .unwrap_or(0.0);
                    cands.sort_by(|a, b| {
                        (a.outputs[&var] - t)
                            .abs()
                            .partial_cmp(&(b.outputs[&var] - t).abs())
                            .unwrap()
                    })
                }
                None => {}
            }
            let best = cands[0];
            // the conclusion's level is the weakest level among the supporting runs
            let level = cands
                .iter()
                .take(1)
                .map(|r| r.evidence)
                .next()
                .unwrap_or(EvidenceLevel::UnverifiedHypothesis);
            let hyps: Vec<HypothesisId> = self
                .hypotheses
                .iter()
                .filter(|h| h.status == HypothesisStatus::Supported)
                .map(|h| h.id)
                .collect();
            let mut caveats = vec![];
            if self
                .contradictions
                .iter()
                .any(|c| c.variable == var && !c.resolved)
            {
                caveats.push("des contradictions non résolues existent sur cette grandeur".into());
            }
            if level == EvidenceLevel::Simulated {
                caveats.push("résultat simulé : non confronté à des observations".into());
            }
            conclusions.push(Conclusion {
                statement: format!(
                    "{} : `{var}` = {:.6e} [{unit}] ({})",
                    match dir {
                        Some(Direction::Minimize) => "Meilleure valeur (minimum)",
                        Some(Direction::Maximize) => "Meilleure valeur (maximum)",
                        Some(Direction::Target) => "Valeur la plus proche de la cible",
                        None => "Valeur déterminée",
                    },
                    best.outputs[&var],
                    level.label_fr()
                ),
                variable: var.clone(),
                value: best.outputs[&var],
                unit,
                level,
                supporting_runs: vec![best.run_id],
                validations: best.validation.into_iter().collect(),
                hypotheses: hyps,
                caveats,
            });
        }
        self.conclusions = conclusions;
        self.log(
            "concluded",
            &format!("{} conclusions", self.conclusions.len()),
            vec![],
        );
    }

    pub fn hypotheses_by_status(&self) -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for h in &self.hypotheses {
            m.entry(format!("{:?}", h.status))
                .or_default()
                .push(h.statement.clone());
        }
        m
    }

    pub fn report_markdown(&self) -> String {
        let mut s = format!("# Campagne {} — {}\n\n", self.id, self.problem.title);
        s.push_str(&self.analysis.to_markdown());
        s.push('\n');
        s.push_str(&self.programme.to_markdown());
        s.push_str("\n## Hypothèses\n");
        for (st, list) in self.hypotheses_by_status() {
            s.push_str(&format!("- **{st}**\n"));
            for l in list {
                s.push_str(&format!("  - {l}\n"));
            }
        }
        s.push_str("\n## Résultats des expériences\n");
        for o in &self.outcomes {
            let name = self
                .programme
                .experiments
                .iter()
                .find(|e| e.id == o.experiment)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            s.push_str(&format!(
                "- {name} : {} runs, {} ; gain informatif {:.3e}\n",
                o.runs.len(),
                if o.succeeded() {
                    "critères satisfaits"
                } else {
                    "critères NON satisfaits"
                },
                o.information_gain
            ));
        }
        if !self.contradictions.is_empty() {
            s.push_str("\n## Contradictions\n");
            for c in &self.contradictions {
                s.push_str(&format!(
                    "- `{}` : {:.6e} vs {:.6e} (écart {:.1} %) — {}\n",
                    c.variable,
                    c.value_a,
                    c.value_b,
                    100.0 * c.relative_gap,
                    if c.resolved {
                        "résolue"
                    } else {
                        "non résolue"
                    }
                ));
            }
        }
        s.push_str("\n## Conclusions\n");
        for c in &self.conclusions {
            s.push_str(&format!(
                "- {}\n  - runs : {}\n  - validations : {}\n",
                c.statement,
                c.supporting_runs
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                c.validations
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            for cv in &c.caveats {
                s.push_str(&format!("  - réserve : {cv}\n"));
            }
        }
        s.push_str(&format!(
            "\n**Arrêt** : {:?} — runs {} / {}, coût {:.2}, temps {:.1} s\n",
            self.stop_reason,
            self.runs_used,
            self.problem.budget.max_runs,
            self.cost_used,
            self.time_used_s
        ));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyse;
    use crate::catalog::StaticCatalog;
    use crate::hypotheses::formulate;
    use crate::programme::plan;

    fn problem() -> (ProblemSpec, ProblemAnalysis, Vec<Hypothesis>, Programme) {
        let m = ComponentManifest::in_process("beam", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_inputs(vec![Port::scalar("length", "m"), Port::scalar("load", "N")])
            .with_outputs(vec![Port::scalar("deflection", "m")]);
        let mut p = ProblemSpec::new("t", "d");
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
        let a = analyse(&p, &StaticCatalog::new(vec![m])).unwrap();
        let hs = formulate(&p, &a);
        let prog = plan(&p, &a, &hs);
        (p, a, hs, prog)
    }

    struct FakeExec {
        gain: Vec<f64>,
        fail_reference: bool,
        tamper: bool,
    }
    impl ExperimentExecutor for FakeExec {
        fn run(
            &mut self,
            planned: &PlannedExperiment,
            _ctl: &ExecutionControl,
        ) -> Result<ExperimentOutcome> {
            let gain = self.gain.pop().unwrap_or(0.0);
            let passed = !(self.fail_reference
                && planned.purpose == crate::programme::ExperimentPurpose::ReferenceCheck);
            let mut runs = vec![];
            for i in 0..planned.n_points.max(1) {
                let mut inputs = indexmap::IndexMap::new();
                inputs.insert("length".into(), 1.0 + i as f64 * 0.1);
                let mut outputs = indexmap::IndexMap::new();
                outputs.insert("deflection".into(), 0.01 * (1.0 + i as f64 * 0.1).powi(3));
                runs.push(RunSummary {
                    run_id: RunId::new(),
                    inputs,
                    outputs,
                    evidence: EvidenceLevel::Computed,
                    validation: Some(ValidationId::new()),
                    valid: true,
                    error: None,
                });
            }
            Ok(ExperimentOutcome {
                experiment: planned.id,
                runs,
                criteria: planned
                    .success_criteria
                    .iter()
                    .map(|c| (c.name.clone(), passed))
                    .collect(),
                criteria_hash: if self.tamper {
                    "x".into()
                } else {
                    planned.criteria_hash.clone()
                },
                information_gain: gain,
                cost: 1.0,
                wall_time_s: 0.5,
            })
        }
    }

    #[test]
    fn completes_programme_and_concludes_with_traceable_runs() {
        let (p, a, hs, prog) = problem();
        let mut c = Campaign::new(p, a, hs, prog);
        let mut ex = FakeExec {
            gain: vec![1.0; 10],
            fail_reference: false,
            tamper: false,
        };
        let r = c.execute(&mut ex, &ExecutionControl::default()).unwrap();
        assert_eq!(r, StopReason::ProgrammeCompleted);
        assert_eq!(c.outcomes.len(), 4);
        let concl = &c.conclusions[0];
        assert_eq!(concl.variable, "deflection");
        assert_eq!(concl.supporting_runs.len(), 1);
        assert_eq!(concl.validations.len(), 1);
        assert!((concl.value - 0.01).abs() < 1e-12);
        assert!(c
            .hypotheses
            .iter()
            .any(|h| h.status == HypothesisStatus::Supported));
        assert!(c.events.iter().any(|e| e.kind == "criteria_fixed"));
        assert!(c.report_markdown().contains("Conclusions"));
    }

    #[test]
    fn stops_on_budget() {
        let (mut p, a, hs, prog) = problem();
        p.budget.max_runs = 1;
        let mut c = Campaign::new(p, a, hs, prog);
        let mut ex = FakeExec {
            gain: vec![1.0; 10],
            fail_reference: false,
            tamper: false,
        };
        assert_eq!(
            c.execute(&mut ex, &ExecutionControl::default()).unwrap(),
            StopReason::BudgetRunsExhausted
        );
    }

    #[test]
    fn stops_when_unproductive() {
        let (p, a, hs, prog) = problem();
        let mut c = Campaign::new(p, a, hs, prog);
        c.rules.max_unproductive = 2;
        let mut ex = FakeExec {
            gain: vec![0.0; 10],
            fail_reference: false,
            tamper: false,
        };
        assert_eq!(
            c.execute(&mut ex, &ExecutionControl::default()).unwrap(),
            StopReason::NoInformationGain
        );
        assert_eq!(c.outcomes.len(), 2);
    }

    #[test]
    fn failed_reference_blocks_dependents_and_refutes_hypothesis() {
        let (p, a, hs, prog) = problem();
        let mut c = Campaign::new(p, a, hs, prog);
        let mut ex = FakeExec {
            gain: vec![1.0; 10],
            fail_reference: true,
            tamper: false,
        };
        assert_eq!(
            c.execute(&mut ex, &ExecutionControl::default()).unwrap(),
            StopReason::PrerequisiteFailed
        );
        assert_eq!(c.outcomes.len(), 1);
    }

    #[test]
    fn tampered_criteria_are_refused() {
        let (p, a, hs, prog) = problem();
        let mut c = Campaign::new(p, a, hs, prog);
        let mut ex = FakeExec {
            gain: vec![1.0; 10],
            fail_reference: false,
            tamper: true,
        };
        assert!(matches!(
            c.execute(&mut ex, &ExecutionControl::default()),
            Err(SimorphError::Protected(_))
        ));
    }

    #[test]
    fn cancellation_stops() {
        let (p, a, hs, prog) = problem();
        let mut c = Campaign::new(p, a, hs, prog);
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        let mut ex = FakeExec {
            gain: vec![1.0; 10],
            fail_reference: false,
            tamper: false,
        };
        assert_eq!(c.execute(&mut ex, &ctl).unwrap(), StopReason::Cancelled);
    }
}
