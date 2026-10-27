//! Model assembly from a problem and the registry.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_model_registry::{ChainPlan, Formalism, ModelDescriptor, Registry};
use simorph_validation::{check_dimensional_against_problem, CheckResult};
use std::collections::BTreeSet;

/// A checked connection between a producer (a chain step or the problem's
/// knowns, named `problem`) and a consumer port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub producer: String,
    pub out_port: String,
    pub consumer: String,
    pub in_port: String,
    /// Multiplicative factor producer-unit → consumer-unit (1 when identical).
    pub conversion_factor: f64,
    /// Additive offset after scaling (non-zero only for affine temperature scales).
    pub conversion_offset: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelAssembly {
    pub problem_id: ProblemId,
    pub descriptors: Vec<ModelDescriptor>,
    /// Composite descriptor of the whole chain (last of `descriptors` is a copy).
    pub composite: ModelDescriptor,
    pub chain: ChainPlan,
    pub connections: Vec<Connection>,
    pub dimensional_report: Vec<CheckResult>,
    /// Required variables no component can produce.
    pub unresolved: Vec<String>,
}

impl ModelAssembly {
    pub fn is_complete(&self) -> bool {
        self.unresolved.is_empty() && self.dimensional_report.iter().all(|c| c.passed)
    }
}

pub struct ModelFactory<'a> {
    registry: &'a Registry,
}

/// Infer the formalism of a component from its capabilities.
pub fn formalism_of(m: &ComponentManifest) -> Formalism {
    use Capability::*;
    let has = |f: &dyn Fn(&Capability) -> bool| m.capabilities.iter().any(f);
    if has(&|c| matches!(c, OdeIntegration)) {
        Formalism::Ode
    } else if has(&|c| matches!(c, DaeIntegration)) {
        Formalism::Dae
    } else if has(&|c| matches!(c, PdeSolver)) {
        Formalism::Pde
    } else if has(&|c| matches!(c, DiscreteEvent)) {
        Formalism::DiscreteEvent
    } else if has(&|c| matches!(c, MultiAgent)) {
        Formalism::MultiAgent
    } else if has(&|c| matches!(c, PhysicsEngine)) {
        Formalism::PhysicalNumerical
    } else if has(&|c| matches!(c, ProbabilisticInference)) {
        Formalism::Probabilistic
    } else if has(&|c| matches!(c, Training | Inference)) {
        Formalism::DataDriven
    } else if has(&|c| matches!(c, FormalVerification)) {
        Formalism::Logical
    } else if has(&|c| matches!(c, CosimulationSlave)) {
        Formalism::Hybrid
    } else if has(&|c| {
        matches!(
            c,
            AlgebraicSolver
                | LinearAlgebra
                | SymbolicMath
                | Optimisation
                | UncertaintyQuantification
        )
    }) {
        Formalism::Algebraic
    } else {
        Formalism::Other("unknown".into())
    }
}

/// Descriptor of a component manifest, with parameters filled from the knowns.
pub fn descriptor_from_manifest(m: &ComponentManifest, knowns: &[Quantity]) -> ModelDescriptor {
    let mut d = ModelDescriptor::new(&m.name, &m.version, formalism_of(m))
        .with_domains(
            &m.supported_domains
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        )
        .with_inputs(m.input_schema.clone())
        .with_outputs(m.output_schema.clone())
        .with_component(m.id);
    d.parameters = m
        .input_schema
        .iter()
        .filter_map(|p| knowns.iter().find(|q| q.name == p.name).cloned())
        .collect();
    d.assumptions = m.assumptions.clone();
    d.validity_domain = m.validity_domain.clone();
    d.uncertainty = m.uncertainty_model.clone();
    for inv in &m.validation_protocol.invariants {
        d = d.with_criterion("invariant", inv);
    }
    for c in &m.validation_protocol.reference_cases {
        d = d.with_criterion(
            &format!("reference:{}", c.name),
            &format!(
                "reproduces `{}` ({}) within rtol={} atol={}",
                c.name, c.source, c.rtol, c.atol
            ),
        );
    }
    d.description = Some(format!(
        "Generated from component manifest `{}` ({})",
        m.name, m.provider
    ));
    d
}

impl<'a> ModelFactory<'a> {
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Assemble the model chain producing the objective, constraint and
    /// unknown variables of the problem from its knowns and design variables.
    pub fn assemble(&self, problem: &ProblemSpec) -> Result<ModelAssembly> {
        problem.validate_structure()?;
        let available: Vec<(String, String)> = problem
            .knowns
            .iter()
            .map(|q| (q.name.clone(), q.unit.clone()))
            .chain(
                problem
                    .design_variables
                    .iter()
                    .map(|d| (d.name.clone(), d.unit.clone())),
            )
            .collect();
        let available_names: Vec<&str> = available.iter().map(|(n, _)| n.as_str()).collect();
        let mut required: BTreeSet<String> = BTreeSet::new();
        for o in &problem.objectives {
            required.insert(o.variable.clone());
        }
        for c in &problem.constraints {
            required.insert(c.variable.clone());
        }
        for u in &problem.unknowns {
            required.insert(u.name.clone());
        }
        required.retain(|r| !available_names.contains(&r.as_str()));
        let required_vec: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
        let chain = self.registry.plan_chain(&required_vec, &available_names);

        // Connections, in chain order: each input of a step is fed either by
        // the problem (known/design variable) or by an earlier step's output.
        let mut connections = vec![];
        let mut report = vec![];
        let mut produced: IndexMap<String, (ComponentManifest, String)> = IndexMap::new();
        let mut descriptors = vec![];
        for step in &chain.steps {
            let m = self
                .registry
                .manifest(step.component)
                .ok_or_else(|| SimorphError::NotFound(format!("manifest {}", step.component)))?
                .clone();
            for p in &m.input_schema {
                if let Some((prod_m, prod_name)) = produced.get(&p.name) {
                    match Registry::check_compatibility(prod_m, &p.name, &m, &p.name) {
                        Ok(()) => {
                            let out_unit = &prod_m
                                .output_schema
                                .iter()
                                .find(|q| q.name == p.name)
                                .unwrap()
                                .unit;
                            let (f, off) = affine(out_unit, &p.unit)?;
                            connections.push(Connection {
                                producer: prod_name.clone(),
                                out_port: p.name.clone(),
                                consumer: m.name.clone(),
                                in_port: p.name.clone(),
                                conversion_factor: f,
                                conversion_offset: off,
                            });
                            report.push(CheckResult::pass(
                                "connection",
                                format!(
                                    "{prod_name}.{} → {}.{} (×{f:.6e})",
                                    p.name, m.name, p.name
                                ),
                            ));
                        }
                        Err(e) => report.push(CheckResult::fail("connection", e.to_string())),
                    }
                } else if let Some((_, unit)) = available.iter().find(|(n, _)| n == &p.name) {
                    match simorph_units::check_same_dimension(unit, &p.unit) {
                        Ok(()) => {
                            let (f, off) = affine(unit, &p.unit)?;
                            connections.push(Connection {
                                producer: "problem".into(),
                                out_port: p.name.clone(),
                                consumer: m.name.clone(),
                                in_port: p.name.clone(),
                                conversion_factor: f,
                                conversion_offset: off,
                            });
                            report.push(CheckResult::pass(
                                "connection",
                                format!(
                                    "problem.{} [{unit}] → {}.{} [{}] (×{f:.6e})",
                                    p.name, m.name, p.name, p.unit
                                ),
                            ));
                        }
                        Err(e) => report.push(CheckResult::fail(
                            "connection",
                            format!("problem.{} → {}.{}: {e}", p.name, m.name, p.name),
                        )),
                    }
                } else {
                    report.push(CheckResult::fail(
                        "connection",
                        format!(
                            "input `{}` of `{}` is neither known nor produced by an earlier step",
                            p.name, m.name
                        ),
                    ));
                }
            }
            for p in &m.output_schema {
                produced.insert(p.name.clone(), (m.clone(), m.name.clone()));
            }
            // objectives/constraints this step produces: dimensional check
            let mut sub = problem.clone();
            sub.objectives
                .retain(|o| m.output_schema.iter().any(|p| p.name == o.variable));
            sub.constraints
                .retain(|c| m.output_schema.iter().any(|p| p.name == c.variable));
            if !sub.objectives.is_empty() || !sub.constraints.is_empty() {
                report.push(check_dimensional_against_problem(&m, &sub));
            }
            // unknowns: unit check against the producing port
            for u in problem
                .unknowns
                .iter()
                .filter(|u| m.output_schema.iter().any(|p| p.name == u.name))
            {
                let port = m.output_schema.iter().find(|p| p.name == u.name).unwrap();
                match simorph_units::check_same_dimension(&u.unit, &port.unit) {
                    Ok(()) => report.push(CheckResult::pass(
                        "unknown_unit",
                        format!(
                            "`{}` [{}] produced by `{}` as [{}]",
                            u.name, u.unit, m.name, port.unit
                        ),
                    )),
                    Err(e) => report.push(CheckResult::fail(
                        "unknown_unit",
                        format!("`{}`: {e}", u.name),
                    )),
                }
            }
            descriptors.push(descriptor_from_manifest(&m, &problem.knowns));
        }
        // Composite descriptor
        let formalisms: BTreeSet<String> = descriptors
            .iter()
            .map(|d| format!("{:?}", d.formalism))
            .collect();
        let composite_formalism = if descriptors.is_empty() {
            Formalism::Other("empty".into())
        } else if formalisms.len() == 1 {
            descriptors[0].formalism.clone()
        } else {
            Formalism::Hybrid
        };
        let mut composite = ModelDescriptor::new(
            &format!("assembly:{}", problem.title),
            "0.1.0",
            composite_formalism,
        )
        .with_domains(
            &problem
                .domains
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        composite.inputs = available
            .iter()
            .filter(|(n, _)| {
                connections
                    .iter()
                    .any(|c| c.producer == "problem" && &c.out_port == n)
            })
            .map(|(n, u)| Port::scalar(n, u))
            .collect();
        composite.outputs = required
            .iter()
            .filter_map(|r| {
                produced.get(r).map(|(m, _)| {
                    m.output_schema
                        .iter()
                        .find(|p| &p.name == r)
                        .unwrap()
                        .clone()
                })
            })
            .collect();
        composite.parameters = problem.knowns.clone();
        composite.dependencies = descriptors.iter().map(|d| d.id).collect();
        composite.assumptions = descriptors
            .iter()
            .flat_map(|d| d.assumptions.iter().map(|a| format!("[{}] {a}", d.name)))
            .collect();
        for d in &descriptors {
            for (k, v) in &d.validity_domain.ranges {
                composite
                    .validity_domain
                    .ranges
                    .insert(format!("{}.{k}", d.name), *v);
            }
        }
        composite.uncertainty = if descriptors
            .iter()
            .all(|d| matches!(d.uncertainty, UncertaintyModel::None))
        {
            UncertaintyModel::None
        } else {
            UncertaintyModel::Described {
                description:
                    "composition of the step uncertainty models; propagate with simorph-uncertainty"
                        .into(),
            }
        };
        for c in &connections {
            composite = composite.with_equation(
                &format!("{}.{}", c.consumer, c.in_port),
                &format!(
                    "{} * {}.{} + {}",
                    c.conversion_factor, c.producer, c.out_port, c.conversion_offset
                ),
            );
        }
        composite.description = Some(format!(
            "{} step(s) producing {:?}",
            descriptors.len(),
            required
        ));
        let unresolved = chain.unsatisfied.clone();
        Ok(ModelAssembly {
            problem_id: problem.id,
            descriptors,
            composite,
            chain,
            connections,
            dimensional_report: report,
            unresolved,
        })
    }
}

/// Affine conversion (factor, offset) from unit `from` to unit `to`.
fn affine(from: &str, to: &str) -> Result<(f64, f64)> {
    let zero = simorph_units::convert(0.0, from, to)?;
    let one = simorph_units::convert(1.0, from, to)?;
    Ok((one - zero, zero))
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_plugin_engineering as eng;
    use simorph_plugin_physics as phys;

    fn registry() -> Registry {
        let mut r = Registry::new();
        for m in phys::manifests().into_iter().chain(eng::manifests()) {
            r.register_manifest(m).unwrap();
        }
        // a heater producing `heat_in_0` from voltage and current, to force a chain
        let heater = ComponentManifest::in_process("test.heater", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_inputs(vec![
                Port::scalar("voltage", "V"),
                Port::scalar("current", "A"),
            ])
            .with_outputs(vec![Port::scalar("heat_in_0", "kW")]);
        r.register_manifest(heater).unwrap();
        r
    }

    #[test]
    fn assembles_two_step_chain_with_conversion_and_hybrid_formalism() {
        let r = registry();
        let mut p = ProblemSpec::new("thermal chain", "d");
        p.domains.push("thermal".into());
        for (n, v, u) in [
            ("T_env", 20.0, "degC"),
            ("C_0", 1000.0, "J/K"),
            ("C_1", 1000.0, "J/K"),
            ("G_env_0", 1.0, "W/K"),
            ("G_env_1", 1.0, "W/K"),
            ("heat_in_1", 0.0, "W"),
            ("T0_0", 20.0, "degC"),
            ("T0_1", 20.0, "degC"),
            ("G_0_1", 5.0, "W/K"),
            ("t_end", 1.0, "h"),
            ("voltage", 12.0, "V"),
            ("current", 2.0, "A"),
        ] {
            p.knowns.push(Quantity::new(n, v, u));
        }
        p.unknowns.push(Unknown {
            name: "final_temperature_0".into(),
            unit: "degC".into(),
            prior: None,
            description: None,
        });
        let a = ModelFactory::new(&r).assemble(&p).unwrap();
        assert!(
            a.is_complete(),
            "{:?}",
            a.dimensional_report
                .iter()
                .filter(|c| !c.passed)
                .collect::<Vec<_>>()
        );
        assert_eq!(a.chain.steps.len(), 2);
        assert_eq!(a.composite.formalism, Formalism::Hybrid);
        let kw = a
            .connections
            .iter()
            .find(|c| c.out_port == "heat_in_0")
            .unwrap();
        assert!((kw.conversion_factor - 1000.0).abs() < 1e-9);
        let tenv = a
            .connections
            .iter()
            .find(|c| c.out_port == "T_env")
            .unwrap();
        assert!((tenv.conversion_offset - 273.15).abs() < 1e-9);
        let h = a
            .connections
            .iter()
            .find(|c| c.out_port == "t_end")
            .unwrap();
        assert!((h.conversion_factor - 3600.0).abs() < 1e-9);
        assert_eq!(a.descriptors.len(), 2);
        assert!(a.descriptors.iter().any(|d| d.formalism == Formalism::Ode));
        assert!(a
            .descriptors
            .iter()
            .any(|d| d.formalism == Formalism::Algebraic));
    }

    #[test]
    fn dimensional_issue_and_unresolved_are_reported() {
        let r = registry();
        let mut p = ProblemSpec::new("beam", "d");
        p.knowns.push(Quantity::new("load", 1.0, "kN"));
        p.knowns.push(Quantity::new("youngs_modulus", 210.0, "GPa"));
        p.knowns.push(Quantity::new("second_moment", 1e-6, "s")); // wrong dimension
        p.design_variables.push(DesignVariable {
            name: "length".into(),
            unit: "mm".into(),
            low: 1.0,
            high: 2.0,
            integer: false,
            initial: None,
        });
        p.objectives.push(Objective {
            name: "o".into(),
            direction: Direction::Minimize,
            variable: "tip_deflection".into(),
            unit: "kg".into(),
            target: None,
            weight: 1.0,
        });
        p.unknowns.push(Unknown {
            name: "lifetime".into(),
            unit: "yr".into(),
            prior: None,
            description: None,
        });
        let a = ModelFactory::new(&r).assemble(&p).unwrap();
        assert!(!a.is_complete());
        assert_eq!(a.unresolved, vec!["lifetime".to_string()]);
        let fails: Vec<&CheckResult> = a.dimensional_report.iter().filter(|c| !c.passed).collect();
        assert!(fails
            .iter()
            .any(|c| c.name == "connection" && c.details.contains("second_moment")));
        assert!(fails.iter().any(|c| c.name == "dimensional_consistency"));
    }
}
