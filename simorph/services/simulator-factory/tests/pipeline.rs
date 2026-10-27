use serde_json::json;
use simorph_core::*;
use simorph_model_registry::Registry;
use simorph_plugin_physics as phys;
use simorph_plugin_sdk::ComponentSet;
use simorph_provenance::Ledger;
use simorph_simulation::Solver;
use simorph_svc_simulator_factory::*;
use std::sync::Arc;

fn factory() -> SimulatorFactory {
    let mut reg = Registry::new();
    let set = ComponentSet::new();
    for c in phys::components() {
        reg.register_manifest(c.manifest().clone()).unwrap();
        set.register_instance(Arc::from(c)).unwrap();
    }
    SimulatorFactory::new(reg, set, Ledger::in_memory())
}

fn thermal_knowns() -> Vec<Quantity> {
    [
        ("T_env", 20.0, "degC"),
        ("C_0", 1000.0, "J/K"),
        ("C_1", 1000.0, "J/K"),
        ("heat_in_0", 10.0, "W"),
        ("heat_in_1", 0.0, "W"),
        ("G_env_0", 1.0, "W/K"),
        ("G_env_1", 1.0, "W/K"),
        ("T0_0", 20.0, "degC"),
        ("T0_1", 20.0, "degC"),
        ("G_0_1", 5.0, "W/K"),
    ]
    .iter()
    .map(|(n, v, u)| Quantity::new(*n, *v, *u))
    .collect()
}

fn decay_spec() -> EquationSpec {
    EquationSpec {
        equations: vec!["d(x)/dt = -k*x".into()],
        variables: vec![Port::scalar("x", "mol")],
        parameters: vec![Quantity::new("k", 0.5, "1/s")],
        reference_cases: vec![ReferenceCase {
            name: "exponential decay".into(),
            inputs: json!({"k": 0.5, "x_0": 2.0, "t_end": 4.0}),
            expected_outputs: json!({"final_x": 2.0 * (-2.0f64).exp()}),
            rtol: 1e-6,
            atol: 1e-9,
            source: "analytic solution x(t) = x0 exp(-k t)".into(),
        }],
        domains: vec!["chemistry".into()],
    }
}

#[test]
fn existing_thermal_component_full_pipeline() {
    let f = factory();
    let ledger = f.ledger.clone();
    let mut req = SimulatorRequest::new("thermal-2-nodes", &["final_temperature_0"]);
    req.knowns = thermal_knowns();
    req.domains = vec!["thermal".into()];
    req.horizon = 2000.0;
    req.target_relative_error = 1e-6;
    let rep = f.run(&req, &ExecutionControl::default()).unwrap();
    assert_eq!(rep.steps.len(), 10, "{}", rep.markdown());
    assert_eq!(
        rep.steps.iter().map(|s| s.step).collect::<Vec<_>>(),
        (1..=10).collect::<Vec<u8>>()
    );
    assert!(rep
        .step(1)
        .unwrap()
        .details
        .iter()
        .any(|d| d.contains("physics.thermal.network")));
    assert!(rep.step(1).unwrap().is_done() && rep.step(2).unwrap().is_done());
    assert!(
        rep.step(2)
            .unwrap()
            .details
            .iter()
            .any(|d| d.contains("T_env") && d.contains("2.931500e2")),
        "{:?}",
        rep.step(2).unwrap().details
    );
    assert!(matches!(
        rep.step(6).unwrap().outcome,
        StepOutcome::Skipped { .. }
    ));
    assert!(rep.step(7).unwrap().is_done(), "{}", rep.markdown());
    let mc = rep.method_comparison.as_ref().expect("comparison");
    assert_eq!(mc.rows.len(), 5);
    let sel = mc.selected.expect("a method meets the target");
    let best = mc.rows.iter().find(|r| r.solver == sel).unwrap();
    assert!(best.relative_error <= 1e-6);
    for r in mc.rows.iter().filter(|r| r.ok && r.relative_error <= 1e-6) {
        assert!(
            best.rhs_evals <= r.rhs_evals,
            "{:?} cheaper than selected {:?}",
            r.solver,
            sel
        );
    }
    assert!(
        rep.steps
            .iter()
            .all(|s| !matches!(s.outcome, StepOutcome::Failed { .. })),
        "{}",
        rep.markdown()
    );
    let name = rep.component_name.clone().unwrap();
    let c = f.components.get(&name).expect("registered");
    let m = c.manifest();
    assert!(matches!(
        m.uncertainty_model,
        UncertaintyModel::RelativeErrorBound { .. }
    ));
    assert!(m.assumptions.iter().any(|a| a.contains("horizon")));
    // the registered component runs with the selected configuration
    let mut r = TaskRequest::new(m.id);
    for q in thermal_knowns() {
        let port = m.input_schema.iter().find(|p| p.name == q.name).unwrap();
        r.inputs.insert(
            q.name.clone(),
            json!(simorph_units::convert(q.value, &q.unit, &port.unit).unwrap()),
        );
    }
    r.inputs.insert("t_end".into(), json!(2000.0));
    let res = run_component(c.as_ref(), &r, &ExecutionControl::default()).unwrap();
    let t0 = res.get_f64("final_temperature_0").unwrap();
    assert!(t0 > 293.15 && t0 < 310.0, "{t0}");
    assert_eq!(res.component, m.id);
    ledger.verify_chain().unwrap();
    assert!(rep.markdown().contains("Comparaison des méthodes"));
}

#[test]
fn generation_from_equations_validated_against_analytic_reference() {
    let f = factory();
    let mut req = SimulatorRequest::new("decay", &["final_x"]);
    req.knowns = vec![
        Quantity::new("k", 0.5, "1/s"),
        Quantity::new("x_0", 2.0, "mol"),
    ];
    req.horizon = 4.0;
    req.equations = Some(decay_spec());
    let rep = f.run(&req, &ExecutionControl::default()).unwrap();
    assert_eq!(rep.steps.len(), 10);
    // `final_x` is also produced by the registered orbit model, which is
    // incompatible with the knowns: the factory must fall back to the equations.
    assert!(
        rep.step(1)
            .unwrap()
            .details
            .iter()
            .any(|d| d.contains("physics.orbit.two_body")),
        "{:?}",
        rep.step(1)
    );
    assert!(rep.step(2).unwrap().is_done());
    assert!(
        rep.step(2)
            .unwrap()
            .details
            .iter()
            .any(|d| d.contains("bascule vers la génération")),
        "{:?}",
        rep.step(2)
    );
    assert!(rep
        .step(3)
        .unwrap()
        .details
        .iter()
        .any(|d| d.contains("non raide")));
    assert!(rep.step(6).unwrap().is_done(), "{}", rep.markdown());
    assert!(rep.step(7).unwrap().is_done(), "{}", rep.markdown());
    let v = rep.validation.as_ref().unwrap();
    assert!(v
        .checks
        .iter()
        .any(|c| c.name.starts_with("reference_case:") && c.passed));
    assert_eq!(v.verdict, EvidenceLevel::Simulated);
    let m = rep.manifest.as_ref().unwrap();
    assert_eq!(
        m.provenance.generated_by.as_deref(),
        Some("simorph-simulator-factory")
    );
    let mc = rep.method_comparison.as_ref().unwrap();
    assert!(mc.selected.is_some());
    assert!(rep.succeeded(), "{}", rep.markdown());
    let c = f
        .components
        .get(rep.component_name.as_ref().unwrap())
        .unwrap();
    let r = TaskRequest::new(c.manifest().id)
        .input("k", 0.5)
        .input("x_0", 2.0)
        .input("t_end", 4.0);
    let res = run_component(c.as_ref(), &r, &ExecutionControl::default()).unwrap();
    let exact = 2.0 * (-2.0f64).exp();
    assert!((res.get_f64("final_x").unwrap() - exact).abs() / exact < 1e-6);
    assert_eq!(res.evidence, EvidenceLevel::Simulated);
}

#[test]
fn generation_through_python_bridge() {
    if let Err(reason) = simorph_adapter_python_bridge::python_with_numpy_available() {
        eprintln!("SKIPPED generation_through_python_bridge: {reason}");
        return;
    }
    let f = factory();
    let mut req = SimulatorRequest::new("decay-py", &["final_x"]);
    req.knowns = vec![
        Quantity::new("k", 0.5, "1/s"),
        Quantity::new("x_0", 2.0, "mol"),
    ];
    req.horizon = 4.0;
    let mut spec = decay_spec();
    spec.reference_cases.clear();
    req.equations = Some(spec);
    req.prefer_python = true;
    let rep = f.run(&req, &ExecutionControl::default()).unwrap();
    assert!(rep.step(6).unwrap().is_done(), "{}", rep.markdown());
    let (path, sha) = rep.generated_script.as_ref().unwrap();
    assert!(std::path::Path::new(path).is_file());
    assert_eq!(sha.len(), 64);
    assert!(rep
        .step(7)
        .unwrap()
        .details
        .iter()
        .any(|d| d.contains("NON VALIDÉ")));
    assert!(matches!(
        rep.step(8).unwrap().outcome,
        StepOutcome::Skipped { .. }
    ));
    let m = rep.manifest.as_ref().unwrap();
    assert_eq!(
        m.provenance.generated_by.as_deref(),
        Some("simorph-simulator-factory")
    );
    assert!(m.assumptions.iter().any(|a| a.contains("NON VALIDÉ")));
    assert!(matches!(
        m.execution_interface,
        ExecutionInterface::Subprocess { .. }
    ));
    let c = f
        .components
        .get(rep.component_name.as_ref().unwrap())
        .unwrap();
    let r = TaskRequest::new(c.manifest().id)
        .input("k", 0.5)
        .input("x_0", 2.0)
        .input("t_end", 4.0);
    let res = run_component(c.as_ref(), &r, &ExecutionControl::default()).unwrap();
    let exact = 2.0 * (-2.0f64).exp();
    assert!(
        (res.get_f64("final_x").unwrap() - exact).abs() / exact < 1e-6,
        "{}",
        res.get_f64("final_x").unwrap()
    );
    assert!(f
        .ledger
        .all()
        .iter()
        .any(|r| r.payload.get("sha256").and_then(|v| v.as_str()) == Some(sha.as_str())));
    let _ = std::fs::remove_dir_all(&f.workdir);
}

#[test]
fn unproducible_output_fails_at_step_6_with_reason() {
    let f = factory();
    let req = SimulatorRequest::new("impossible", &["plasma_temperature"]);
    let rep = f.run(&req, &ExecutionControl::default()).unwrap();
    assert_eq!(rep.steps.len(), 10);
    match &rep.step(6).unwrap().outcome {
        StepOutcome::Failed { reason } => assert!(
            reason.contains("plasma_temperature") && reason.contains("aucune équation"),
            "{reason}"
        ),
        o => panic!("expected Failed, got {o:?}"),
    }
    for n in [7u8, 8, 9, 10] {
        assert!(matches!(
            rep.step(n).unwrap().outcome,
            StepOutcome::Skipped { .. }
        ));
    }
    assert!(rep.component_name.is_none() && rep.manifest.is_none());
    assert!(!rep.succeeded());
}

#[test]
fn stiff_model_switches_to_implicit_in_step_3() {
    let f = factory();
    let mut req = SimulatorRequest::new("stiff-forced", &["final_x"]);
    req.horizon = 10.0;
    req.target_relative_error = 1e-4;
    req.knowns = vec![
        Quantity::new("lam", 1000.0, "1/s"),
        Quantity::new("w", 1.0, "1/s"),
        Quantity::new("x_0", 0.0, "1"),
    ];
    req.equations = Some(EquationSpec {
        equations: vec!["d(x)/dt = -lam*(x - cos(w*t))".into()],
        variables: vec![Port::scalar("x", "1")],
        parameters: vec![
            Quantity::new("lam", 1000.0, "1/s"),
            Quantity::new("w", 1.0, "1/s"),
        ],
        reference_cases: vec![],
        domains: vec![],
    });
    let rep = f.run(&req, &ExecutionControl::default()).unwrap();
    let s3 = rep.step(3).unwrap();
    assert!(s3.is_done());
    assert!(
        s3.details
            .iter()
            .any(|d| d.contains("raide") && d.contains("implicite")),
        "{:?}",
        s3.details
    );
    let m = rep.manifest.as_ref().expect("component produced");
    assert!(m.assumptions.iter().any(|a| a.contains("stiff")));
    let mc = rep.method_comparison.as_ref().unwrap();
    let euler = mc.rows.iter().find(|r| r.solver == Solver::Euler).unwrap();
    assert!(
        !(euler.ok && euler.relative_error <= req.target_relative_error),
        "explicit Euler must not pass on a stiff problem: {euler:?}"
    );
    let trap = mc
        .rows
        .iter()
        .find(|r| r.solver == Solver::Trapezoidal)
        .unwrap();
    assert!(trap.ok, "{trap:?}");
}
