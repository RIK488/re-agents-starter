//! Method selection: SIMORPH chooses optimisation methods from the
//! problem's features instead of applying one algorithm systematically.

use crate::problem::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Continuous,
    Integer,
    Mixed,
    /// Sequential / combinatorial decisions (tree search).
    Combinatorial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCost {
    /// < 1 ms
    Cheap,
    /// ~ seconds
    Moderate,
    /// minutes to hours (each evaluation is precious)
    Expensive,
}

/// Features describing an optimisation problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProblemFeatures {
    pub dimension: usize,
    pub variables: VariableKind,
    pub has_gradient: bool,
    /// Objective is believed smooth (continuous derivatives).
    pub smooth: bool,
    pub noisy: bool,
    /// Many local optima suspected.
    pub multimodal: bool,
    pub n_objectives: usize,
    pub n_constraints: usize,
    pub evaluation_cost: EvaluationCost,
    /// Available evaluation budget.
    pub budget: usize,
    /// Uncertain parameters whose effect must be hedged (robust optimisation).
    pub uncertain_parameters: bool,
    /// The problem is sequential decision-making with state (RL).
    pub sequential_decisions: bool,
}

impl Default for ProblemFeatures {
    fn default() -> Self {
        Self {
            dimension: 2,
            variables: VariableKind::Continuous,
            has_gradient: false,
            smooth: true,
            noisy: false,
            multimodal: false,
            n_objectives: 1,
            n_constraints: 0,
            evaluation_cost: EvaluationCost::Cheap,
            budget: 10_000,
            uncertain_parameters: false,
            sequential_decisions: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MethodRecommendation {
    pub method: Method,
    /// Higher is better; 0 means unsuitable.
    pub score: f64,
    pub justification: Vec<String>,
}

/// Rule-based, auditable method selector.
pub struct MethodSelector;

impl MethodSelector {
    pub fn recommend(f: &ProblemFeatures) -> Vec<MethodRecommendation> {
        let mut out: Vec<MethodRecommendation> = vec![];
        fn push(out: &mut Vec<MethodRecommendation>, m: Method, score: f64, why: Vec<&str>) {
            out.push(MethodRecommendation {
                method: m,
                score,
                justification: why.into_iter().map(String::from).collect(),
            })
        }
        let d = f.dimension as f64;
        let exp = f.evaluation_cost == EvaluationCost::Expensive;
        let low_dim = f.dimension <= 20;

        if f.sequential_decisions {
            push(
                &mut out,
                Method::QLearning,
                8.0,
                vec!["sequential decision problem with state: reinforcement learning"],
            );
            push(
                &mut out,
                Method::MonteCarloTreeSearch,
                7.0,
                vec!["sequential decisions can be searched as a tree"],
            );
            return rank(out);
        }
        if f.variables == VariableKind::Combinatorial {
            push(
                &mut out,
                Method::MonteCarloTreeSearch,
                8.0,
                vec!["combinatorial decision space: MCTS with rollouts"],
            );
            push(
                &mut out,
                Method::BeamSearch,
                6.0,
                vec!["combinatorial with a good heuristic bound: beam search is cheap"],
            );
            push(
                &mut out,
                Method::SimulatedAnnealing,
                4.0,
                vec!["annealing as a generic fallback for discrete spaces"],
            );
            return rank(out);
        }
        if f.uncertain_parameters {
            push(
                &mut out,
                Method::RobustExpectation,
                9.0,
                vec!["uncertain parameters: optimise the expectation over parameter samples"],
            );
            push(
                &mut out,
                Method::RobustCvar,
                8.0,
                vec!["uncertain parameters with risk aversion: CVaR of the objective"],
            );
        }
        if f.n_objectives >= 2 {
            push(&mut out, Method::Nsga2, 10.0, vec!["multiple objectives: NSGA-II returns the Pareto front instead of a single scalarised point"]);
            if exp {
                push(
                    &mut out,
                    Method::BayesianOptimization,
                    5.0,
                    vec![
                        "expensive multi-objective: scalarised BO is possible but loses the front",
                    ],
                );
            }
            return rank(out);
        }
        // single objective
        if exp && low_dim {
            let mut why =
                vec!["expensive evaluations: a GP surrogate makes every evaluation count"];
            if f.budget > 300 {
                why.push("budget above ~300 evaluations: GP cost O(n^3) grows, consider batching or trust-region BO");
            }
            push(
                &mut out,
                Method::BayesianOptimization,
                10.0 - if f.budget > 300 { 2.0 } else { 0.0 },
                why,
            );
        } else if exp {
            push(
                &mut out,
                Method::BayesianOptimization,
                4.0,
                vec!["expensive but high-dimensional: plain GP-BO degrades beyond ~20 dimensions"],
            );
        }
        if f.has_gradient && f.smooth && !f.noisy && f.variables == VariableKind::Continuous {
            push(&mut out, Method::Lbfgs, 10.0, vec!["analytic gradient on a smooth deterministic objective: quasi-Newton converges superlinearly"]);
            push(
                &mut out,
                Method::GradientDescent,
                5.0,
                vec!["gradient available; steepest descent is a robust but slower baseline"],
            );
        } else if f.smooth
            && !f.noisy
            && !f.multimodal
            && f.variables == VariableKind::Continuous
            && !exp
        {
            push(&mut out, Method::Lbfgs, 7.0 - (d / 100.0).min(3.0), vec!["smooth deterministic objective: L-BFGS with numerical gradient (2d evaluations per gradient)"]);
            push(
                &mut out,
                Method::NelderMead,
                if f.dimension <= 10 { 8.0 } else { 3.0 },
                vec!["derivative-free local search; efficient below ~10 dimensions"],
            );
        } else if !f.noisy
            && f.dimension <= 10
            && !f.multimodal
            && f.variables == VariableKind::Continuous
        {
            push(
                &mut out,
                Method::NelderMead,
                6.0,
                vec!["derivative-free local search for low dimension"],
            );
        }
        if f.noisy || f.multimodal || !f.smooth || f.variables != VariableKind::Continuous {
            let mut why_de = vec![];
            let mut why_cma = vec![];
            if f.noisy {
                why_de.push("noisy objective: population methods average out noise");
                why_cma.push("noisy objective: CMA-ES ranks rather than differences values");
            }
            if f.multimodal {
                why_de.push("multimodal landscape: differential evolution explores globally");
                why_cma.push("multimodal landscape: CMA-ES with IPOP restarts");
            }
            if !f.smooth {
                why_de.push("non-smooth objective: no gradient needed");
                why_cma.push("non-smooth objective: no gradient needed");
            }
            if f.variables != VariableKind::Continuous {
                why_de.push("integer/mixed variables handled by rounding in DE");
            }
            let base = if exp { 3.0 } else { 9.0 };
            push(
                &mut out,
                Method::DifferentialEvolution,
                base + if f.variables != VariableKind::Continuous {
                    1.0
                } else {
                    0.0
                },
                why_de,
            );
            push(
                &mut out,
                Method::CmaEs,
                base - if f.variables != VariableKind::Continuous {
                    2.0
                } else {
                    0.0
                } + if f.dimension > 5 { 0.5 } else { 0.0 },
                why_cma,
            );
            push(
                &mut out,
                Method::SimulatedAnnealing,
                base - 4.0,
                vec!["annealing: simple global fallback, slower convergence"],
            );
        }
        if f.n_constraints > 0 {
            for r in out.iter_mut() {
                match r.method {
                    Method::DifferentialEvolution | Method::CmaEs => {
                        r.score += 1.0;
                        r.justification.push(
                            "constraints handled natively with Deb's feasibility rules".to_string(),
                        );
                    }
                    Method::Lbfgs
                    | Method::GradientDescent
                    | Method::NelderMead
                    | Method::BayesianOptimization => {
                        r.score -= 1.0;
                        r.justification.push(
                            "constraints handled by quadratic penalty (may need tuning)"
                                .to_string(),
                        );
                    }
                    _ => {}
                }
            }
        }
        if out.is_empty() {
            push(
                &mut out,
                Method::RandomSearch,
                1.0,
                vec!["no feature matched; random search baseline"],
            );
        }
        rank(out)
    }
}

fn rank(mut v: Vec<MethodRecommendation>) -> Vec<MethodRecommendation> {
    v.retain(|r| r.score > 0.0);
    v.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expensive_low_dim_prefers_bo() {
        let f = ProblemFeatures {
            dimension: 4,
            evaluation_cost: EvaluationCost::Expensive,
            budget: 60,
            ..Default::default()
        };
        assert_eq!(
            MethodSelector::recommend(&f)[0].method,
            Method::BayesianOptimization
        );
    }

    #[test]
    fn noisy_multimodal_prefers_population_methods() {
        let f = ProblemFeatures {
            dimension: 10,
            noisy: true,
            multimodal: true,
            smooth: false,
            ..Default::default()
        };
        let r = MethodSelector::recommend(&f);
        assert!(
            matches!(r[0].method, Method::DifferentialEvolution | Method::CmaEs),
            "{:?}",
            r[0]
        );
        assert!(matches!(
            r[1].method,
            Method::DifferentialEvolution | Method::CmaEs
        ));
    }

    #[test]
    fn multi_objective_prefers_nsga2() {
        let f = ProblemFeatures {
            n_objectives: 3,
            ..Default::default()
        };
        assert_eq!(MethodSelector::recommend(&f)[0].method, Method::Nsga2);
    }

    #[test]
    fn smooth_with_gradient_prefers_lbfgs() {
        let f = ProblemFeatures {
            dimension: 50,
            has_gradient: true,
            ..Default::default()
        };
        let r = MethodSelector::recommend(&f);
        assert_eq!(r[0].method, Method::Lbfgs);
        assert!(!r[0].justification.is_empty());
    }

    #[test]
    fn combinatorial_and_sequential() {
        let f = ProblemFeatures {
            variables: VariableKind::Combinatorial,
            ..Default::default()
        };
        assert_eq!(
            MethodSelector::recommend(&f)[0].method,
            Method::MonteCarloTreeSearch
        );
        let f = ProblemFeatures {
            sequential_decisions: true,
            ..Default::default()
        };
        assert_eq!(MethodSelector::recommend(&f)[0].method, Method::QLearning);
    }

    #[test]
    fn uncertain_parameters_prefer_robust() {
        let f = ProblemFeatures {
            uncertain_parameters: true,
            multimodal: true,
            ..Default::default()
        };
        assert_eq!(
            MethodSelector::recommend(&f)[0].method,
            Method::RobustExpectation
        );
    }
}
