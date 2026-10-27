//! Regression tests for the 2026-10-27 engines audit.

use indexmap::IndexMap;
use simorph_core::{Capability, ComponentManifest, ExecutionControl, Port};
use simorph_cosimulation::{run, AlgebraicSlave, CosimSlave, Coupling, MasterAlgorithm, MasterOptions};
use simorph_simulation::Params;

fn affine(name: &str) -> AlgebraicSlave {
    // y = 0.5 x + 1 (direct feedthrough); fixed point of x = y is 2.
    let m = ComponentManifest::in_process(name, "1", "t")
        .with_capabilities(vec![Capability::CosimulationSlave])
        .with_inputs(vec![Port::scalar("x", "1")])
        .with_outputs(vec![Port::scalar("y", "1")]);
    AlgebraicSlave::new(m, |v| Ok(IndexMap::from([("y".to_string(), 0.5 * v["x"] + 1.0)])))
}

fn constant(name: &str) -> AlgebraicSlave {
    let m = ComponentManifest::in_process(name, "1", "t")
        .with_capabilities(vec![Capability::CosimulationSlave])
        .with_outputs(vec![Port::scalar("y", "1")]);
    AlgebraicSlave::new(m, |_| Ok(IndexMap::from([("y".to_string(), 1.0)])))
}

#[test]
fn audit_self_loop_through_feedthrough_is_an_algebraic_loop() {
    let mut c = Coupling::builder().add_slave("a", Box::new(affine("a")), Params::new()).unwrap().connect("a.y", "a.x").unwrap().build().unwrap();
    assert_eq!(c.algebraic_loops, vec![vec!["a".to_string()]]);
    // Non-iterative masters must refuse instead of returning a lagged value.
    assert!(run(&mut c, &MasterOptions::new(MasterAlgorithm::Jacobi, 1.0, 0.1), &ExecutionControl::default()).is_err());
    let tr = run(&mut c, &MasterOptions::new(MasterAlgorithm::IterativeGaussSeidel { max_iter: 100, tol: 1e-12 }, 1.0, 0.1), &ExecutionControl::default()).unwrap();
    assert!((tr.final_value("a.y").unwrap() - 2.0).abs() < 1e-9);
}

#[test]
fn audit_final_sample_recorded_with_record_every() {
    let mut c = Coupling::builder().add_slave("k", Box::new(constant("k")), Params::new()).unwrap().build().unwrap();
    let tr = run(&mut c, &MasterOptions::new(MasterAlgorithm::GaussSeidel, 1.0, 0.1).with_record_every(3), &ExecutionControl::default()).unwrap();
    assert_eq!(*tr.times.last().unwrap(), 1.0);
    assert_eq!(tr.macro_steps, 10);
}

#[test]
fn audit_unconnected_inputs_require_a_value() {
    let err = Coupling::builder().add_slave("a", Box::new(affine("a")), Params::new()).unwrap().build();
    assert!(err.is_err());
    let mut p = Params::new();
    p.insert("x".into(), 3.0);
    assert!(Coupling::builder().add_slave("a", Box::new(affine("a")), p).unwrap().build().is_ok());
    let c = Coupling::builder().add_slave("a", Box::new(affine("a")), Params::new()).unwrap().allow_default_input("a.x", 4.0).unwrap().build().unwrap();
    assert_eq!(c.unconnected_inputs, vec!["a.x".to_string()]);
}

#[test]
fn audit_algebraic_slave_rejects_unknown_inputs() {
    let mut s = affine("a");
    s.initialize(0.0, &Params::from([("x".to_string(), 0.0)])).unwrap();
    assert!(s.set_input("x", 1.0).is_ok());
    assert!(s.set_input("typo", 1.0).is_err());
}
