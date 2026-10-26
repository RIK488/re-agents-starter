//! Verification of the co-simulation masters.

use indexmap::IndexMap;
use simorph_core::{Capability, ComponentManifest, ExecutionControl, Port, SimorphError};
use simorph_cosimulation::{
    run, run_ensemble, AlgebraicSlave, Coupling, MasterAlgorithm, MasterOptions, OdeSlave,
};
use simorph_simulation::model::param;
use simorph_simulation::{simulate, DynamicalModel, Params, Solver, SolverOptions};
use std::sync::Arc;

fn p(pairs: &[(&str, f64)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Thermal mass: C dT/dt = heat_in - G (T - T_env).
struct Thermal;
impl DynamicalModel for Thermal {
    fn name(&self) -> &str {
        "test.thermal"
    }
    fn state_dim(&self) -> usize {
        1
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("T".into(), "K".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("C".into(), "J/K".into()),
            ("G".into(), "W/K".into()),
            ("T_env".into(), "K".into()),
            ("T0".into(), "K".into()),
            ("heat_in".into(), "W".into()),
        ]
    }
    fn initial_state(&self, params: &Params) -> Vec<f64> {
        vec![param(params, "T0").unwrap()]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], pr: &Params) -> simorph_core::Result<()> {
        dx[0] = (param(pr, "heat_in")? - param(pr, "G")? * (x[0] - param(pr, "T_env")?))
            / param(pr, "C")?;
        Ok(())
    }
}

/// Monolithic closed loop: heater P = clamp(kp (T_set - T), 0, Pmax).
struct ClosedLoop;
impl DynamicalModel for ClosedLoop {
    fn name(&self) -> &str {
        "test.closedloop"
    }
    fn state_dim(&self) -> usize {
        1
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("T".into(), "K".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("C".into(), "J/K".into()),
            ("G".into(), "W/K".into()),
            ("T_env".into(), "K".into()),
            ("T0".into(), "K".into()),
            ("kp".into(), "W/K".into()),
            ("T_set".into(), "K".into()),
            ("P_max".into(), "W".into()),
        ]
    }
    fn initial_state(&self, params: &Params) -> Vec<f64> {
        vec![param(params, "T0").unwrap()]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], pr: &Params) -> simorph_core::Result<()> {
        let pw = (param(pr, "kp")? * (param(pr, "T_set")? - x[0])).clamp(0.0, param(pr, "P_max")?);
        dx[0] = (pw - param(pr, "G")? * (x[0] - param(pr, "T_env")?)) / param(pr, "C")?;
        Ok(())
    }
}

/// Controller slave outputting power in kW (unit conversion exercised).
fn controller(power_unit: &str) -> AlgebraicSlave {
    let m = ComponentManifest::in_process("test.controller", "0.1.0", "test")
        .with_capabilities(vec![Capability::CosimulationSlave])
        .with_inputs(vec![Port::scalar("T", "K")])
        .with_outputs(vec![Port::scalar("power", power_unit)]);
    let scale = if power_unit == "kW" { 1e-3 } else { 1.0 };
    AlgebraicSlave::new(m, move |v| {
        let pw = (param(v, "kp")? * (param(v, "T_set")? - param(v, "T")?))
            .clamp(0.0, param(v, "P_max")?);
        Ok(IndexMap::from([("power".to_string(), pw * scale)]))
    })
}

const PARAMS: [(&str, f64); 7] = [
    ("C", 1000.0),
    ("G", 10.0),
    ("T_env", 273.15),
    ("T0", 273.15),
    ("kp", 100.0),
    ("T_set", 300.0),
    ("P_max", 2000.0),
];

fn thermal_coupling(power_unit: &str, thermal_params: &Params) -> Coupling {
    let th = OdeSlave::new(
        Arc::new(Thermal),
        &["heat_in"],
        SolverOptions::new(Solver::Rk45).with_tolerances(1e-10, 1e-12),
        false,
    )
    .unwrap();
    Coupling::builder()
        .add_slave(
            "ctrl",
            Box::new(controller(power_unit)),
            p(&[("kp", 100.0), ("T_set", 300.0), ("P_max", 2000.0)]),
        )
        .unwrap()
        .add_slave("thermal", Box::new(th), thermal_params.clone())
        .unwrap()
        .connect("ctrl.power", "thermal.heat_in")
        .unwrap()
        .connect("thermal.T", "ctrl.T")
        .unwrap()
        .build()
        .unwrap()
}

fn monolithic(t_end: f64) -> f64 {
    let tr = simulate(
        &ClosedLoop,
        &p(&PARAMS),
        None,
        (0.0, t_end),
        &SolverOptions::new(Solver::Rk45).with_tolerances(1e-10, 1e-12),
        &ExecutionControl::default(),
    )
    .unwrap();
    tr.final_state()[0]
}

#[test]
fn coupled_thermal_matches_monolithic_and_converges_with_macro_step() {
    let t_end = 60.0;
    let reference = monolithic(t_end);
    let thermal_params = p(&[
        ("C", 1000.0),
        ("G", 10.0),
        ("T_env", 273.15),
        ("T0", 273.15),
    ]);
    let mut errs = vec![];
    for h in [0.4, 0.2, 0.1] {
        let mut gs = thermal_coupling("W", &thermal_params);
        let tr = run(
            &mut gs,
            &MasterOptions::new(MasterAlgorithm::GaussSeidel, t_end, h),
            &ExecutionControl::default(),
        )
        .unwrap();
        let t_gs = tr.final_value("thermal.T").unwrap();
        let mut jc = thermal_coupling("W", &thermal_params);
        let tr = run(
            &mut jc,
            &MasterOptions::new(MasterAlgorithm::Jacobi, t_end, h),
            &ExecutionControl::default(),
        )
        .unwrap();
        let t_j = tr.final_value("thermal.T").unwrap();
        assert!(tr.algebraic_loops.is_empty());
        errs.push((
            (t_gs - reference).abs(),
            (t_j - reference).abs(),
            (t_gs - t_j).abs(),
        ));
    }
    // Both masters approach the monolithic solution and each other as h -> 0.
    for w in errs.windows(2) {
        assert!(
            w[1].0 < w[0].0 && w[1].1 < w[0].1 && w[1].2 < w[0].2,
            "errors not decreasing: {errs:?}"
        );
    }
    let (e_gs, e_j, _) = errs[2];
    assert!(
        e_gs < 0.05 && e_j < 0.05,
        "h=0.1: gs err {e_gs} K, jacobi err {e_j} K (T≈{reference} K)"
    );
    // Step doubling keeps the error estimate below 1 and does not break the result.
    let mut sd = thermal_coupling("W", &thermal_params);
    let tr = run(
        &mut sd,
        &MasterOptions::new(MasterAlgorithm::GaussSeidel, t_end, 0.5)
            .with_step_doubling(1e-3, 1e-3),
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(tr.max_error_estimate <= 1.0);
    assert!((tr.final_value("thermal.T").unwrap() - reference).abs() < 0.05);
}

#[test]
fn unit_conversion_between_slaves() {
    let thermal_params = p(&[
        ("C", 1000.0),
        ("G", 10.0),
        ("T_env", 273.15),
        ("T0", 273.15),
    ]);
    let mut w = thermal_coupling("W", &thermal_params);
    let mut kw = thermal_coupling("kW", &thermal_params);
    let o = MasterOptions::new(MasterAlgorithm::GaussSeidel, 30.0, 0.1);
    let a = run(&mut w, &o, &ExecutionControl::default()).unwrap();
    let b = run(&mut kw, &o, &ExecutionControl::default()).unwrap();
    let ta = a.final_value("thermal.T").unwrap();
    let tb = b.final_value("thermal.T").unwrap();
    assert!((ta - tb).abs() < 1e-9, "{ta} vs {tb}");
    assert!(
        (a.final_value("ctrl.power").unwrap() - 1000.0 * b.final_value("ctrl.power").unwrap())
            .abs()
            < 1e-9
    );
}

#[test]
fn dimensional_mismatch_is_rejected_at_build() {
    let th = OdeSlave::new(
        Arc::new(Thermal),
        &["heat_in"],
        SolverOptions::default(),
        false,
    )
    .unwrap();
    let bad = ComponentManifest::in_process("bad", "1", "t")
        .with_outputs(vec![Port::scalar("power", "J")]);
    let r = Coupling::builder()
        .add_slave(
            "bad",
            Box::new(AlgebraicSlave::new(bad, |_| Ok(IndexMap::new()))),
            Params::new(),
        )
        .unwrap()
        .add_slave("thermal", Box::new(th), Params::new())
        .unwrap()
        .connect("bad.power", "thermal.heat_in");
    assert!(
        matches!(r, Err(SimorphError::Dimensional(_))),
        "{:?}",
        r.err().map(|e| e.to_string())
    );
    // Unknown port.
    let th = OdeSlave::new(
        Arc::new(Thermal),
        &["heat_in"],
        SolverOptions::default(),
        false,
    )
    .unwrap();
    let r = Coupling::builder()
        .add_slave("thermal", Box::new(th), Params::new())
        .unwrap()
        .connect("thermal.nope", "thermal.heat_in");
    assert!(matches!(r, Err(SimorphError::NotFound(_))));
}

fn gain(name: &str, a: f64, b: f64) -> AlgebraicSlave {
    // y = a * u + b
    let m = ComponentManifest::in_process(name, "1", "t")
        .with_inputs(vec![Port::scalar("u", "1")])
        .with_outputs(vec![Port::scalar("y", "1")]);
    AlgebraicSlave::new(m, move |v| {
        Ok(IndexMap::from([("y".to_string(), a * param(v, "u")? + b)]))
    })
}

#[test]
fn algebraic_loop_detected_and_solved_iteratively() {
    // y1 = 0.5 y2 + 1 ; y2 = 0.5 y1 + 1  =>  y1 = y2 = 2.
    let build = || {
        Coupling::builder()
            .add_slave("a", Box::new(gain("a", 0.5, 1.0)), Params::new())
            .unwrap()
            .add_slave("b", Box::new(gain("b", 0.5, 1.0)), Params::new())
            .unwrap()
            .connect("a.y", "b.u")
            .unwrap()
            .connect("b.y", "a.u")
            .unwrap()
            .build()
            .unwrap()
    };
    let c = build();
    assert_eq!(c.algebraic_loops.len(), 1);
    assert_eq!(c.algebraic_loops[0].len(), 2);
    let mut c = build();
    let r = run(
        &mut c,
        &MasterOptions::new(MasterAlgorithm::GaussSeidel, 1.0, 0.5),
        &ExecutionControl::default(),
    );
    assert!(matches!(r, Err(SimorphError::Incompatible(_))));
    let mut c = build();
    let tr = run(
        &mut c,
        &MasterOptions::new(
            MasterAlgorithm::IterativeGaussSeidel {
                max_iter: 100,
                tol: 1e-10,
            },
            1.0,
            0.5,
        ),
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!((tr.final_value("a.y").unwrap() - 2.0).abs() < 1e-8);
    assert!((tr.final_value("b.y").unwrap() - 2.0).abs() < 1e-8);
    assert!(tr.fixed_point_iterations > 2);
}

#[test]
fn ensemble_propagates_parameter_samples() {
    let samples: Vec<Params> = [800.0, 1000.0, 1200.0]
        .iter()
        .map(|&c| p(&[("C", c), ("G", 10.0), ("T_env", 273.15), ("T0", 273.15)]))
        .collect();
    let trajs = run_ensemble(
        |s| Ok(thermal_coupling("W", s)),
        &samples,
        &MasterOptions::new(MasterAlgorithm::GaussSeidel, 20.0, 0.2),
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(trajs.len(), 3);
    let t: Vec<f64> = trajs
        .iter()
        .map(|t| t.final_value("thermal.T").unwrap())
        .collect();
    // Larger thermal mass heats more slowly.
    assert!(t[0] > t[1] && t[1] > t[2], "{t:?}");
}

#[test]
fn consistency_checks_name_the_slave_and_time() {
    let m =
        ComponentManifest::in_process("nan", "1", "t").with_outputs(vec![Port::scalar("y", "1")]);
    let s = AlgebraicSlave::new(m, |_| Ok(IndexMap::from([("y".to_string(), f64::NAN)])));
    let mut c = Coupling::builder()
        .add_slave("nanslave", Box::new(s), Params::new())
        .unwrap()
        .build()
        .unwrap();
    let r = run(
        &mut c,
        &MasterOptions::new(MasterAlgorithm::Jacobi, 1.0, 0.5),
        &ExecutionControl::default(),
    );
    let msg = format!("{}", r.err().unwrap());
    assert!(
        msg.contains("nanslave") && msg.contains("non-finite"),
        "{msg}"
    );
}

#[test]
fn non_feedthrough_sources_are_stepped_before_their_consumers() {
    // src (state-based) -> a (algebraic) -> th (state-based) -> a : the
    // loop th<->a is broken on th's outgoing edge, but src must still be
    // stepped before a so that a sees src's fresh output.
    let m = ComponentManifest::in_process("sum", "1", "t")
        .with_inputs(vec![Port::scalar("u", "K"), Port::scalar("v", "K")])
        .with_outputs(vec![Port::scalar("y", "W")]);
    let a = AlgebraicSlave::new(m, |p| {
        Ok(IndexMap::from([(
            "y".to_string(),
            param(p, "u")? + param(p, "v")?,
        )]))
    });
    let src = OdeSlave::new(Arc::new(Thermal), &[], SolverOptions::default(), false).unwrap();
    let th = OdeSlave::new(
        Arc::new(Thermal),
        &["heat_in"],
        SolverOptions::default(),
        false,
    )
    .unwrap();
    let tp = p(&[
        ("C", 1000.0),
        ("G", 10.0),
        ("T_env", 273.15),
        ("T0", 273.15),
        ("heat_in", 0.0),
    ]);
    let c = Coupling::builder()
        .add_slave("a", Box::new(a), Params::new())
        .unwrap()
        .add_slave("th", Box::new(th), tp.clone())
        .unwrap()
        .add_slave("src", Box::new(src), tp)
        .unwrap()
        .connect("src.T", "a.u")
        .unwrap()
        .connect("th.T", "a.v")
        .unwrap()
        .connect("a.y", "th.heat_in")
        .unwrap()
        .build()
        .unwrap();
    assert!(c.algebraic_loops.is_empty());
    let names: Vec<&str> = c.order.iter().map(|&i| c.slaves[i].name.as_str()).collect();
    let pos = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(pos("src") < pos("a"), "{names:?}");
    assert!(pos("a") < pos("th"), "{names:?}");
}
