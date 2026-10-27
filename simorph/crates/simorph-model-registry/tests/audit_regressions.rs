//! Regression tests for defects found by the engine audit.

use simorph_core::{Capability, ComponentManifest, Port, Quantity};
use simorph_model_registry::*;

fn comp(name: &str, ins: Vec<Port>, outs: Vec<Port>) -> ComponentManifest {
    ComponentManifest::in_process(name, "1", "t")
        .with_capabilities(vec![Capability::AlgebraicSolver])
        .with_inputs(ins)
        .with_outputs(outs)
}

#[test]
fn audit_chain_matches_name_and_dimension() {
    let mut r = Registry::new();
    // produces `T` as a length (wrong for the consumer)
    r.register_manifest(comp(
        "lengthy",
        vec![Port::scalar("x", "m")],
        vec![Port::scalar("T", "m")],
    ))
    .unwrap();
    r.register_manifest(comp(
        "consumer",
        vec![Port::scalar("T", "K")],
        vec![Port::scalar("q", "W")],
    ))
    .unwrap();
    let plan = r.plan_chain(&["q"], &["x"]);
    assert!(
        !plan.complete(),
        "a length must not feed a temperature: {plan:?}"
    );
    // the right producer makes the chain complete
    r.register_manifest(comp(
        "thermo",
        vec![Port::scalar("x", "m")],
        vec![Port::scalar("T", "degC")],
    ))
    .unwrap();
    let mut r2 = Registry::new();
    r2.register_manifest(comp(
        "thermo",
        vec![Port::scalar("x", "m")],
        vec![Port::scalar("T", "degC")],
    ))
    .unwrap();
    r2.register_manifest(comp(
        "consumer",
        vec![Port::scalar("T", "K")],
        vec![Port::scalar("q", "W")],
    ))
    .unwrap();
    let plan = r2.plan_chain(&["q"], &["x"]);
    assert!(plan.complete(), "{plan:?}");
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["thermo", "consumer"]
    );
    // typed user inputs are checked too
    assert!(!r2.plan_chain_typed(&["q"], &[("x", "s")]).complete());
    assert!(r2.plan_chain_typed(&["q"], &[("x", "km")]).complete());
}

#[test]
fn audit_duplicate_registration_is_refused_unless_replaced() {
    let mut r = Registry::new();
    let a = comp("dup", vec![], vec![Port::scalar("y", "m")]);
    r.register_manifest(a.clone()).unwrap();
    assert!(
        r.register_manifest(a.clone()).is_err(),
        "same id silently replaced"
    );
    let b = comp("dup", vec![], vec![Port::scalar("y", "s")]); // same name@version
    assert!(r.register_manifest(b.clone()).is_err());
    r.replace_manifest(b).unwrap();
    assert_eq!(r.manifests().count(), 1);
    assert_eq!(
        r.find_manifest_by_name("dup").unwrap().output_schema[0].unit,
        "s"
    );

    let m = ModelDescriptor::new("m", "1", Formalism::Algebraic);
    r.register_model(m.clone()).unwrap();
    assert!(r.register_model(m.clone()).is_err());
    r.replace_model(m).unwrap();
    assert_eq!(r.models().count(), 1);
}

#[test]
fn audit_duplicate_dependency_entries_are_not_a_cycle() {
    let mut r = Registry::new();
    let base = ModelDescriptor::new("base", "1", Formalism::Algebraic);
    let base_id = r.register_model(base).unwrap();
    let top = ModelDescriptor::new("top", "1", Formalism::Algebraic)
        .with_dependency(base_id)
        .with_dependency(base_id);
    let top_id = r.register_model(top).unwrap();
    assert_eq!(r.model_dependency_order().unwrap(), vec![base_id, top_id]);
}

#[test]
fn audit_dimensionally_inconsistent_equations_are_refused() {
    let thermal = || {
        ModelDescriptor::new("rc", "1", Formalism::Ode)
            .with_inputs(vec![Port::scalar("P_in", "W"), Port::scalar("T_env", "K")])
            .with_states(vec![Port::scalar("T", "K")])
            .with_parameters(vec![
                Quantity::new("h", 10.0, "W/(m^2*K)"),
                Quantity::new("A", 1.0, "m^2"),
                Quantity::new("C", 1000.0, "J/K"),
            ])
    };
    let mut r = Registry::new();
    // consistent ODE with derivative notation
    let ok = thermal().with_equation("energy", "dT/dt = (P_in - h*A*(T - T_env)) / C");
    let reports = ok.equation_dimension_reports();
    assert_eq!(reports[0].1, EquationCheck::Consistent, "{reports:?}");
    r.register_model(ok).unwrap();
    // forgot the heat capacity: W ≠ K/s
    let bad = ModelDescriptor {
        name: "rc-bad".into(),
        ..thermal()
    }
    .with_equation("energy", "dT/dt = P_in - h*A*(T - T_env)");
    let bad = ModelDescriptor {
        id: simorph_core::ModelId::deterministic("rc-bad"),
        ..bad
    };
    let e = r.register_model(bad).unwrap_err().to_string();
    assert!(e.contains("dimensionally inconsistent"), "{e}");
    // name = lhs, expression = rhs (model-factory style); adding K to W
    let bad2 = ModelDescriptor::new("rc-bad2", "1", Formalism::Algebraic)
        .with_inputs(vec![Port::scalar("P", "W"), Port::scalar("T", "K")])
        .with_outputs(vec![Port::scalar("Q", "W")])
        .with_equation("Q", "P + T");
    assert!(r.register_model(bad2).is_err());
    // unknown symbols cannot be checked: accepted, reported as unchecked
    let unk = ModelDescriptor::new("opaque", "1", Formalism::Algebraic)
        .with_outputs(vec![Port::scalar("y", "m")])
        .with_equation("fit", "y = k * mystery");
    assert!(matches!(
        unk.equation_dimension_reports()[0].1,
        EquationCheck::Unchecked(_)
    ));
    r.register_model(unk).unwrap();
}
