//! Experimental programme: ordered, scored experiments with success
//! criteria fixed before execution.

use crate::analysis::{ProblemAnalysis, QuantityCategory};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentPurpose {
    /// Run the component's declared reference cases (validity of the tool).
    ReferenceCheck,
    /// Screening of design variables (which ones matter).
    Screening,
    /// Global sensitivity / uncertainty propagation.
    Sensitivity,
    /// Optimisation of the objectives over the design variables.
    Optimisation,
    /// High-fidelity verification of the retained solution.
    Verification,
    /// Comparison of prediction with supplied observations.
    Calibration,
    /// Search for counter-examples to a hypothesis / constraint.
    CounterExample,
}

/// A success criterion is an expression over outputs/metrics evaluated by the
/// validation engine.  It is written *before* execution and hashed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuccessCriterion {
    pub name: String,
    pub expression: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedExperiment {
    pub id: ExperimentId,
    pub name: String,
    pub purpose: ExperimentPurpose,
    pub component: ComponentId,
    pub component_name: String,
    pub hypotheses: Vec<HypothesisId>,
    /// Inputs fixed from the problem's knowns (name → value in the problem's unit).
    pub fixed_inputs: indexmap::IndexMap<String, f64>,
    /// Design variables explored by this experiment.
    pub design_variables: Vec<DesignVariable>,
    pub n_points: usize,
    pub fidelity: f64,
    pub success_criteria: Vec<SuccessCriterion>,
    /// Hash of the success criteria at planning time.
    pub criteria_hash: String,
    pub tradeoff: Tradeoff,
    /// Experiments that must have succeeded before this one runs.
    pub prerequisites: Vec<ExperimentId>,
}

impl PlannedExperiment {
    pub fn utility(&self, w: &TradeoffWeights) -> f64 {
        self.tradeoff.utility(w)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Programme {
    pub problem_id: ProblemId,
    pub experiments: Vec<PlannedExperiment>,
    pub weights: TradeoffWeights,
}

impl Programme {
    /// Experiments in planned execution order (prerequisites first, then by utility).
    pub fn ordered(&self) -> Vec<&PlannedExperiment> {
        let mut out: Vec<&PlannedExperiment> = vec![];
        let mut done: Vec<ExperimentId> = vec![];
        let mut remaining: Vec<&PlannedExperiment> = self.experiments.iter().collect();
        while !remaining.is_empty() {
            let mut ready: Vec<&PlannedExperiment> = remaining
                .iter()
                .cloned()
                .filter(|e| e.prerequisites.iter().all(|p| done.contains(p)))
                .collect();
            if ready.is_empty() {
                // broken prerequisites: append the rest in declared order
                out.extend(remaining.iter().cloned());
                break;
            }
            ready.sort_by(|a, b| {
                b.utility(&self.weights)
                    .partial_cmp(&a.utility(&self.weights))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let next = ready[0];
            done.push(next.id);
            out.push(next);
            remaining.retain(|e| e.id != next.id);
        }
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut s = String::from("## Programme expérimental\n\n| # | Expérience | But | Composant | Points | Fidélité | Utilité | Critères |\n|---|---|---|---|---|---|---|---|\n");
        for (i, e) in self.ordered().iter().enumerate() {
            s.push_str(&format!(
                "| {} | {} | {:?} | {} | {} | {:.2} | {:.2} | {} |\n",
                i + 1,
                e.name,
                e.purpose,
                e.component_name,
                e.n_points,
                e.fidelity,
                e.utility(&self.weights),
                e.success_criteria
                    .iter()
                    .map(|c| c.expression.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        s
    }
}

fn criteria_hash(c: &[SuccessCriterion]) -> String {
    simorph_core::hashing::hash_json(&c).expect("serialisable")
}

/// Build the programme from the analysis.
pub fn plan(
    problem: &ProblemSpec,
    analysis: &ProblemAnalysis,
    hypotheses: &[Hypothesis],
) -> Programme {
    let mut experiments = vec![];
    let fixed: indexmap::IndexMap<String, f64> = problem
        .knowns
        .iter()
        .map(|q| (q.name.clone(), q.value))
        .collect();
    let n_design_total = problem.design_variables.len();
    let budget_runs = problem.budget.max_runs.max(1) as usize;

    for cand in &analysis.solution_candidates {
        let related_h: Vec<HypothesisId> = hypotheses
            .iter()
            .filter(|h| h.statement.contains(&cand.name))
            .map(|h| h.id)
            .collect();
        let base_cost = cand.resource_requirements.wall_time_s.max(0.01);
        let produces_target = analysis.assessments.iter().any(|a| {
            cand.produces.contains(&a.name) && !a.categories.contains(&QuantityCategory::Known)
        });
        if !produces_target || !cand.missing_inputs.is_empty() {
            // A component whose inputs are not all available cannot be run
            // directly; it is reached through model chaining (ARC-03).
            continue;
        }

        // Only the design variables that are inputs of this component can be
        // explored with it; the others would be silently ignored.
        let design: Vec<DesignVariable> = problem
            .design_variables
            .iter()
            .filter(|d| cand.requires.contains(&d.name))
            .cloned()
            .collect();
        let n_design = design.len();
        let _ = n_design_total;

        // 1. Reference check: cheap, decisive for tool validity.
        let ref_id = ExperimentId::deterministic(&format!("{}:{}:ref", problem.id, cand.component));
        experiments.push(PlannedExperiment {
            id: ref_id,
            name: format!("Cas de référence — {}", cand.name),
            purpose: ExperimentPurpose::ReferenceCheck,
            component: cand.component,
            component_name: cand.name.clone(),
            hypotheses: related_h.clone(),
            fixed_inputs: fixed.clone(),
            design_variables: vec![],
            n_points: 1,
            fidelity: 1.0,
            success_criteria: vec![SuccessCriterion {
                name: "reference_cases_pass".into(),
                expression: "reference_cases_failed == 0".into(),
                description: "tous les cas de référence déclarés par le composant sont reproduits dans leur tolérance".into(),
            }],
            criteria_hash: String::new(),
            tradeoff: Tradeoff { precision: 1.0, compute_time_s: base_cost, cost: base_cost, risk: 0.05, information_value: 2.0 },
            prerequisites: vec![],
        });

        // 2. Screening / sensitivity when design variables exist.
        if n_design > 0 {
            let n_points = (4 * n_design + 2).min(budget_runs);
            let scr_id =
                ExperimentId::deterministic(&format!("{}:{}:screen", problem.id, cand.component));
            experiments.push(PlannedExperiment {
                id: scr_id,
                name: format!("Criblage des variables — {}", cand.name),
                purpose: ExperimentPurpose::Screening,
                component: cand.component,
                component_name: cand.name.clone(),
                hypotheses: related_h.clone(),
                fixed_inputs: fixed.clone(),
                design_variables: design.clone(),
                n_points,
                fidelity: 0.5,
                success_criteria: vec![SuccessCriterion {
                    name: "all_runs_valid".into(),
                    expression: "failed_runs == 0".into(),
                    description: "aucun run n'a quitté le domaine de validité".into(),
                }],
                criteria_hash: String::new(),
                tradeoff: Tradeoff {
                    precision: 0.5,
                    compute_time_s: base_cost * n_points as f64 * 0.5,
                    cost: base_cost * n_points as f64 * 0.5,
                    risk: 0.1,
                    information_value: 1.5,
                },
                prerequisites: vec![ref_id],
            });

            // 3. Optimisation when objectives exist.
            if !problem.objectives.is_empty() {
                let n_points = (20 * n_design).min(budget_runs);
                let opt_id =
                    ExperimentId::deterministic(&format!("{}:{}:opt", problem.id, cand.component));
                let mut criteria: Vec<SuccessCriterion> = problem
                    .constraints
                    .iter()
                    .map(|c| SuccessCriterion {
                        name: format!("constraint_{}", c.name),
                        expression: match c.kind {
                            ConstraintKind::UpperBound => {
                                format!("{} <= {}", c.variable, c.bound + c.tolerance)
                            }
                            ConstraintKind::LowerBound => {
                                format!("{} >= {}", c.variable, c.bound - c.tolerance)
                            }
                            ConstraintKind::Equality => format!(
                                "abs({} - {}) <= {}",
                                c.variable,
                                c.bound,
                                c.tolerance.max(1e-12)
                            ),
                        },
                        description: format!(
                            "contrainte « {} » respectée par la solution retenue",
                            c.name
                        ),
                    })
                    .collect();
                criteria.push(SuccessCriterion { name: "improvement".into(), expression: "best_objective_improved == 1".into(), description: "l'optimisation améliore l'objectif par rapport au meilleur point du criblage".into() });
                experiments.push(PlannedExperiment {
                    id: opt_id,
                    name: format!("Optimisation — {}", cand.name),
                    purpose: ExperimentPurpose::Optimisation,
                    component: cand.component,
                    component_name: cand.name.clone(),
                    hypotheses: related_h.clone(),
                    fixed_inputs: fixed.clone(),
                    design_variables: design.clone(),
                    n_points,
                    fidelity: 0.7,
                    success_criteria: criteria,
                    criteria_hash: String::new(),
                    tradeoff: Tradeoff {
                        precision: 0.7,
                        compute_time_s: base_cost * n_points as f64 * 0.7,
                        cost: base_cost * n_points as f64 * 0.7,
                        risk: 0.15,
                        information_value: 1.2,
                    },
                    prerequisites: vec![scr_id],
                });
                // 4. Verification at full fidelity.
                experiments.push(PlannedExperiment {
                    id: ExperimentId::deterministic(&format!(
                        "{}:{}:verify",
                        problem.id, cand.component
                    )),
                    name: format!("Vérification haute fidélité — {}", cand.name),
                    purpose: ExperimentPurpose::Verification,
                    component: cand.component,
                    component_name: cand.name.clone(),
                    hypotheses: related_h.clone(),
                    fixed_inputs: fixed.clone(),
                    design_variables: vec![],
                    n_points: 1,
                    fidelity: 1.0,
                    success_criteria: vec![SuccessCriterion {
                        name: "fidelity_consistent".into(),
                        expression: "relative_change_vs_low_fidelity <= 0.05".into(),
                        description:
                            "le résultat haute fidélité confirme le résultat basse fidélité à 5 %"
                                .into(),
                    }],
                    criteria_hash: String::new(),
                    tradeoff: Tradeoff {
                        precision: 1.0,
                        compute_time_s: base_cost * 4.0,
                        cost: base_cost * 4.0,
                        risk: 0.05,
                        information_value: 0.8,
                    },
                    prerequisites: vec![opt_id],
                });
            }
        }
        // 5. Calibration when observations are attached.
        if problem.attachments.iter().any(|a| {
            a.media_type.contains("csv")
                || a.media_type.contains("observation")
                || a.dataset_id.is_some()
        }) {
            experiments.push(PlannedExperiment {
                id: ExperimentId::deterministic(&format!(
                    "{}:{}:calib",
                    problem.id, cand.component
                )),
                name: format!("Comparaison aux observations — {}", cand.name),
                purpose: ExperimentPurpose::Calibration,
                component: cand.component,
                component_name: cand.name.clone(),
                hypotheses: related_h.clone(),
                fixed_inputs: fixed.clone(),
                design_variables: vec![],
                n_points: 1,
                fidelity: 1.0,
                success_criteria: vec![SuccessCriterion {
                    name: "statistically_consistent".into(),
                    expression: "z_score <= 2".into(),
                    description: "les prédictions sont compatibles avec les observations à 2σ"
                        .into(),
                }],
                criteria_hash: String::new(),
                tradeoff: Tradeoff {
                    precision: 1.0,
                    compute_time_s: base_cost,
                    cost: base_cost,
                    risk: 0.1,
                    information_value: 2.5,
                },
                prerequisites: vec![ref_id],
            });
        }
    }
    for e in &mut experiments {
        e.criteria_hash = criteria_hash(&e.success_criteria);
    }
    Programme {
        problem_id: problem.id,
        experiments,
        weights: problem.tradeoff_weights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyse;
    use crate::catalog::StaticCatalog;
    use crate::hypotheses::formulate;

    #[test]
    fn programme_orders_reference_before_optimisation_and_hashes_criteria() {
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
            name: "min deflection".into(),
            direction: Direction::Minimize,
            variable: "deflection".into(),
            unit: "m".into(),
            target: None,
            weight: 1.0,
        });
        p.constraints.push(Constraint {
            name: "max".into(),
            variable: "deflection".into(),
            kind: ConstraintKind::UpperBound,
            bound: 0.01,
            unit: "m".into(),
            tolerance: 0.0,
            hard: true,
        });
        let a = analyse(&p, &StaticCatalog::new(vec![m])).unwrap();
        let hs = formulate(&p, &a);
        let prog = plan(&p, &a, &hs);
        let order: Vec<ExperimentPurpose> = prog.ordered().iter().map(|e| e.purpose).collect();
        assert_eq!(
            order,
            vec![
                ExperimentPurpose::ReferenceCheck,
                ExperimentPurpose::Screening,
                ExperimentPurpose::Optimisation,
                ExperimentPurpose::Verification
            ]
        );
        for e in &prog.experiments {
            assert!(!e.criteria_hash.is_empty());
            assert!(!e.success_criteria.is_empty());
        }
        assert!(prog.experiments[2]
            .success_criteria
            .iter()
            .any(|c| c.expression == "deflection <= 0.01"));
        assert!(!prog.to_markdown().is_empty());
    }
}
