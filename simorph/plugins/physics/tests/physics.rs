//! Verification of the physics models against analytic results and
//! conservation laws.

use simorph_core::{ExecutionControl, TaskRequest};
use simorph_cosimulation::{run, MasterAlgorithm, MasterOptions};
use simorph_plugin_physics::attitude::{self, RigidBody};
use simorph_plugin_physics::battery::Battery;
use simorph_plugin_physics::orbit::{self, TwoBody, J2_EARTH, MU_EARTH, R_EARTH};
use simorph_plugin_physics::thermal::ThermalNetwork;
use simorph_plugin_physics::{demo, solar};
use simorph_simulation::{simulate, Params, Solver, SolverOptions};
use std::f64::consts::PI;

fn p(pairs: &[(&str, f64)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}
fn ctl() -> ExecutionControl {
    ExecutionControl::default()
}
fn tight() -> SolverOptions {
    SolverOptions::new(Solver::Rk45).with_tolerances(1e-11, 1e-9)
}

// ---------------------------------------------------------------- orbit

#[test]
fn circular_orbit_period_matches_kepler() {
    let a = 7.0e6;
    let params = orbit::circular_params(a, 0.0, 0.0);
    let t = TwoBody::circular_period(a, MU_EARTH);
    let tr = simulate(&TwoBody, &params, None, (0.0, t), &tight(), &ctl()).unwrap();
    let xf = tr.final_state();
    let dist = ((xf[0] - a).powi(2) + xf[1].powi(2) + xf[2].powi(2)).sqrt();
    assert!(dist < 5.0, "position error after one period: {dist} m");
    assert!((tr.outputs["altitude"].last().unwrap() - (a - R_EARTH)).abs() < 5.0);
}

#[test]
fn eccentric_orbit_conserves_energy_and_angular_momentum() {
    let a = 8.0e6;
    let e = 0.1;
    let rp = a * (1.0 - e);
    let vp = (MU_EARTH * (2.0 / rp - 1.0 / a)).sqrt();
    let params = p(&[
        ("mu", MU_EARTH),
        ("r_eq", R_EARTH),
        ("j2", 0.0),
        ("sun_x", 1.0),
        ("sun_y", 0.0),
        ("sun_z", 0.0),
        ("x0", rp),
        ("y0", 0.0),
        ("z0", 0.0),
        ("vx0", 0.0),
        ("vy0", vp * 0.8),
        ("vz0", vp * 0.6),
    ]);
    let t = 5.0 * TwoBody::circular_period(a, MU_EARTH);
    let tr = simulate(&TwoBody, &params, None, (0.0, t), &tight(), &ctl()).unwrap();
    let (e0, h0, _, _) = TwoBody::elements(&tr.states[0], MU_EARTH);
    let h0m = (h0[0].powi(2) + h0[1].powi(2) + h0[2].powi(2)).sqrt();
    for s in &tr.states {
        let (en, h, _, _) = TwoBody::elements(s, MU_EARTH);
        assert!(
            ((en - e0) / e0).abs() < 1e-8,
            "energy drift {}",
            (en - e0) / e0
        );
        let hm = (h[0].powi(2) + h[1].powi(2) + h[2].powi(2)).sqrt();
        assert!(((hm - h0m) / h0m).abs() < 1e-8, "|h| drift");
        for k in 0..3 {
            assert!(((h[k] - h0[k]) / h0m).abs() < 1e-8, "h direction drift");
        }
    }
    // Analytic energy -mu/(2a).
    assert!(((e0 + MU_EARTH / (2.0 * a)) / e0).abs() < 1e-10);
}

#[test]
fn j2_nodal_regression_matches_secular_rate() {
    let a = 7.0e6;
    let inc = 45.0f64.to_radians();
    let params = orbit::circular_params(a, inc, J2_EARTH);
    let period = TwoBody::circular_period(a, MU_EARTH);
    let n_orbits = 10.0;
    let tr = simulate(
        &TwoBody,
        &params,
        None,
        (0.0, n_orbits * period),
        &tight(),
        &ctl(),
    )
    .unwrap();
    let (_, _, raan0, _) = TwoBody::elements(&tr.states[0], MU_EARTH);
    let (_, _, raan1, inc1) = TwoBody::elements(tr.final_state(), MU_EARTH);
    let mut d = raan1 - raan0;
    while d > PI {
        d -= 2.0 * PI;
    }
    while d < -PI {
        d += 2.0 * PI;
    }
    let expected =
        TwoBody::j2_nodal_rate(a, 0.0, inc, MU_EARTH, R_EARTH, J2_EARTH) * n_orbits * period;
    assert!(expected < 0.0);
    let rel = ((d - expected) / expected).abs();
    assert!(
        rel < 0.03,
        "RAAN drift {d} rad vs secular {expected} rad (rel {rel})"
    );
    // J2 is conservative: the energy including the J2 potential is conserved;
    // inclination has no secular drift.
    assert!((inc1 - inc).abs() < 2e-3);
    let e: Vec<f64> = tr.outputs["specific_energy"].clone();
    assert!(((e.last().unwrap() - e[0]) / e[0]).abs() < 1e-8);
}

#[test]
fn eclipse_fraction_for_low_earth_orbit() {
    let alt = 500e3;
    let a = R_EARTH + alt;
    let params = orbit::circular_params(a, 0.0, 0.0);
    let period = TwoBody::circular_period(a, MU_EARTH);
    let opts = SolverOptions::new(Solver::Rk4).with_dt(1.0);
    let tr = simulate(&TwoBody, &params, None, (0.0, period), &opts, &ctl()).unwrap();
    let ecl = &tr.outputs["eclipse"];
    let frac = ecl.iter().sum::<f64>() / ecl.len() as f64;
    let expected = (R_EARTH / a).asin() / PI; // 2·asin(R/a) / 2π
    assert!(
        (frac - expected).abs() < 0.01,
        "eclipse fraction {frac} vs {expected}"
    );
}

// -------------------------------------------------------------- thermal

#[test]
fn thermal_steady_state_matches_linear_solve() {
    let net = ThermalNetwork::chain(3, false);
    let params = p(&[
        ("T_env", 280.0),
        ("C_0", 500.0),
        ("C_1", 800.0),
        ("C_2", 300.0),
        ("heat_in_0", 50.0),
        ("heat_in_1", 0.0),
        ("heat_in_2", 10.0),
        ("G_env_0", 1.0),
        ("G_env_1", 0.5),
        ("G_env_2", 2.0),
        ("T0_0", 300.0),
        ("T0_1", 300.0),
        ("T0_2", 300.0),
        ("G_0_1", 3.0),
        ("G_1_2", 1.5),
    ]);
    let ss = net.linear_steady_state(&params).unwrap();
    let tr = simulate(
        &net,
        &params,
        None,
        (0.0, 2.0e4),
        &SolverOptions::new(Solver::Trapezoidal).with_dt(5.0),
        &ctl(),
    )
    .unwrap();
    for (i, t) in tr.final_state().iter().enumerate() {
        assert!(
            (t - ss[i]).abs() < 1e-3,
            "node {i}: {t} vs steady {}",
            ss[i]
        );
    }
    // Energy balance at steady state: heat in = heat out to environment.
    let q_out: f64 = (0..3)
        .map(|i| params[&format!("G_env_{i}")] * (ss[i] - 280.0))
        .sum();
    assert!((q_out - 60.0).abs() < 1e-9);
}

#[test]
fn two_node_transient_is_exponential() {
    let net = ThermalNetwork::chain(2, false);
    let (c, g): (f64, f64) = (1000.0, 5.0);
    let params = p(&[
        ("T_env", 0.0),
        ("C_0", c),
        ("C_1", c),
        ("heat_in_0", 0.0),
        ("heat_in_1", 0.0),
        ("G_env_0", 0.0),
        ("G_env_1", 0.0),
        ("T0_0", 310.0),
        ("T0_1", 290.0),
        ("G_0_1", g),
    ]);
    let tr = simulate(&net, &params, None, (0.0, 400.0), &tight(), &ctl()).unwrap();
    for (t, s) in tr.times.iter().zip(&tr.states) {
        let d = 20.0 * (-2.0 * g * t / c).exp();
        assert!((s[0] - s[1] - d).abs() < 1e-6, "t={t}");
        assert!((s[0] + s[1] - 600.0).abs() < 1e-6, "energy not conserved");
    }
    // Manifest reference case agrees with the model.
    let comp = simorph_plugin_physics::thermal::component(2);
    let case = &comp.manifest().validation_protocol.reference_cases[0];
    let mut req = TaskRequest::new(comp.manifest().id);
    for (k, v) in case.inputs.as_object().unwrap() {
        req = req.input(k, v.as_f64().unwrap());
    }
    let res = simorph_core::run_component(comp.as_ref(), &req, &ctl()).unwrap();
    for (k, v) in case.expected_outputs.as_object().unwrap() {
        let got = res.get_f64(k).unwrap();
        let exp = v.as_f64().unwrap();
        assert!(
            (got - exp).abs() <= case.atol + case.rtol * exp.abs(),
            "{k}: {got} vs {exp}"
        );
    }
}

#[test]
fn radiative_node_cools_toward_space() {
    let net = ThermalNetwork::chain(1, true);
    let params = p(&[
        ("T_env", 2.7),
        ("T_space", 2.7),
        ("C_0", 100.0),
        ("heat_in_0", 0.0),
        ("G_env_0", 0.0),
        ("T0_0", 350.0),
        ("epsA_0", 0.1),
    ]);
    let tr = simulate(&net, &params, None, (0.0, 3600.0), &tight(), &ctl()).unwrap();
    // Analytic for T_space≈0: 1/T³ - 1/T0³ = 3 εAσ t / C.
    let t_end = tr.final_time();
    let expected = (1.0 / 350.0f64.powi(3)
        + 3.0 * 0.1 * simorph_plugin_physics::thermal::SIGMA * t_end / 100.0)
        .powf(-1.0 / 3.0);
    assert!(
        (tr.final_state()[0] - expected).abs() < 1e-3,
        "{} vs {expected}",
        tr.final_state()[0]
    );
}

// -------------------------------------------------------------- battery

#[test]
fn coulomb_counting_and_energy_balance() {
    let params = p(&[
        ("capacity_ah", 10.0),
        ("r_internal", 0.0),
        ("v_ocv_min", 3.6),
        ("v_ocv_max", 3.6),
        ("soc0", 0.5),
        ("power_in", 36.0),
        ("power_out", 0.0),
    ]);
    let tr = simulate(&Battery, &params, None, (0.0, 1800.0), &tight(), &ctl()).unwrap();
    assert!((tr.final_state()[0] - 1.0).abs() < 1e-9);
    assert!((tr.final_state()[1] - 36.0 * 1800.0).abs() < 1e-6);
    // With resistance and a sloped OCV: ∫P dt = e_stored + e_loss.
    let params = p(&[
        ("capacity_ah", 5.0),
        ("r_internal", 0.05),
        ("v_ocv_min", 3.0),
        ("v_ocv_max", 4.2),
        ("soc0", 0.3),
        ("power_in", 50.0),
        ("power_out", 20.0),
    ]);
    let t_end = 1200.0;
    let tr = simulate(&Battery, &params, None, (0.0, t_end), &tight(), &ctl()).unwrap();
    let xf = tr.final_state();
    let supplied = 30.0 * t_end;
    assert!(
        ((xf[1] + xf[2] - supplied) / supplied).abs() < 1e-8,
        "energy balance"
    );
    assert!(xf[2] > 0.0 && xf[0] > 0.3 && xf[0] < 1.0);
    let v = tr.outputs["voltage"].last().unwrap();
    let i = tr.outputs["current"].last().unwrap();
    assert!((v * i - 30.0).abs() < 1e-9, "terminal power {}", v * i);
    // Excessive discharge is refused, not silently clamped.
    let bad = p(&[
        ("capacity_ah", 5.0),
        ("r_internal", 1.0),
        ("v_ocv_min", 3.0),
        ("v_ocv_max", 4.2),
        ("soc0", 0.5),
        ("power_in", 0.0),
        ("power_out", 1000.0),
    ]);
    assert!(simulate(&Battery, &bad, None, (0.0, 1.0), &tight(), &ctl()).is_err());
}

// ---------------------------------------------------------------- solar

#[test]
fn solar_power_analytic() {
    let pw = solar::power(1000.0, 0.5f64, 320.0, 0.0, 2.0, 0.2, -0.004, 300.0);
    assert!((pw - 1000.0 * 2.0 * 0.2 * (1.0 - 0.08) * 0.5).abs() < 1e-12);
    assert_eq!(
        solar::power(1000.0, 0.5, 320.0, 1.0, 2.0, 0.2, -0.004, 300.0),
        0.0
    );
    assert_eq!(
        solar::power(1000.0, -0.3, 300.0, 0.0, 2.0, 0.2, -0.004, 300.0),
        0.0
    );
    let c = solar::component();
    let case = &c.manifest().validation_protocol.reference_cases[0];
    let mut req = TaskRequest::new(c.manifest().id);
    for (k, v) in case.inputs.as_object().unwrap() {
        req = req.input(k, v.as_f64().unwrap());
    }
    let res = simorph_core::run_component(c.as_ref(), &req, &ctl()).unwrap();
    assert!(
        (res.get_f64("power").unwrap() - case.expected_outputs["power"].as_f64().unwrap()).abs()
            < 1e-9
    );
    assert_eq!(res.evidence, simorph_core::EvidenceLevel::Computed);
}

// ------------------------------------------------------------- attitude

#[test]
fn torque_free_rigid_body_conserves_angular_momentum_and_quaternion_norm() {
    let mut params = attitude::default_params();
    // Spin about the intermediate axis (Ixx = 0.05 lies between Izz and Iyy):
    // the Dzhanibekov flip exercises the full nonlinear coupling.
    params.insert("w_init_x".into(), 0.5);
    params.insert("w_init_y".into(), 0.1);
    params.insert("w_init_z".into(), 0.02);
    let tr = simulate(&RigidBody, &params, None, (0.0, 100.0), &tight(), &ctl()).unwrap();
    let h0 = RigidBody::inertial_angular_momentum(&tr.states[0], &params);
    let h0m = (h0[0].powi(2) + h0[1].powi(2) + h0[2].powi(2)).sqrt();
    for s in &tr.states {
        let h = RigidBody::inertial_angular_momentum(s, &params);
        for k in 0..3 {
            assert!(
                ((h[k] - h0[k]) / h0m).abs() < 1e-7,
                "H drift {:?} vs {:?}",
                h,
                h0
            );
        }
        let n = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2] + s[3] * s[3]).sqrt();
        assert!((n - 1.0).abs() < 1e-7, "quaternion norm {n}");
    }
    // Rotational kinetic energy is conserved too.
    let ke = |s: &[f64]| 0.5 * (0.05 * s[4] * s[4] + 0.06 * s[5] * s[5] + 0.04 * s[6] * s[6]);
    assert!(((ke(tr.final_state()) - ke(&tr.states[0])) / ke(&tr.states[0])).abs() < 1e-7);
    // The body tumbles: wy grows far beyond its initial 0.1 rad/s.
    assert!(tr.state_series(5).iter().cloned().fold(0.0, f64::max) > 0.25);
}

#[test]
fn pd_controller_converges_to_target_attitude() {
    let mut params = attitude::default_params();
    params.insert("ctrl_enabled".into(), 1.0);
    params.insert("kp".into(), 0.02);
    params.insert("kd".into(), 0.1);
    // 90° about x initially, target identity.
    let s = (PI / 4.0).sin();
    params.insert("q_init_0".into(), (PI / 4.0).cos());
    params.insert("q_init_1".into(), s);
    params.insert("w_init_y".into(), 0.05);
    let tr = simulate(&RigidBody, &params, None, (0.0, 200.0), &tight(), &ctl()).unwrap();
    let err0 = RigidBody::pointing_error(&tr.states[0], &params);
    assert!((err0 - PI / 2.0).abs() < 1e-9);
    let errf = RigidBody::pointing_error(tr.final_state(), &params);
    assert!(errf < 1e-3, "pointing error {errf} rad");
    assert!(tr.final_state()[4..].iter().all(|w| w.abs() < 1e-3));
}

// ------------------------------------------------------ discovery & demo

#[test]
fn discovery_and_manifest_integrity() {
    let ms = simorph_plugin_physics::manifests();
    assert_eq!(ms.len(), 6);
    for m in &ms {
        m.validate_structure().unwrap();
        assert!(!m.assumptions.is_empty(), "{} has no assumptions", m.name);
        assert!(
            !m.validation_protocol.reference_cases.is_empty(),
            "{} has no reference case",
            m.name
        );
        assert!(!m.supported_domains.is_empty());
        assert!(simorph_plugin_physics::build(m.id).is_some());
    }
    for name in [
        "physics.orbit.two_body",
        "physics.thermal.network",
        "physics.battery.equivalent_circuit",
        "physics.solar.array",
        "physics.attitude.rigid_body",
    ] {
        assert!(simorph_plugin_physics::slave(name).is_ok(), "{name}");
    }
    assert!(simorph_plugin_physics::slave("physics.nope").is_err());
}

#[test]
fn nanosat_cosimulation_runs_one_orbit() {
    let np = demo::NanosatParams::default();
    let mut c = demo::nanosat_coupling(&np).unwrap();
    assert!(c.algebraic_loops.is_empty());
    let period = TwoBody::circular_period(R_EARTH + np.altitude, MU_EARTH);
    let tr = run(
        &mut c,
        &MasterOptions::new(MasterAlgorithm::GaussSeidel, period, 10.0),
        &ctl(),
    )
    .unwrap();
    let ecl = tr.signal("orbit.eclipse").unwrap();
    let frac = ecl.iter().sum::<f64>() / ecl.len() as f64;
    let expected = (R_EARTH / (R_EARTH + np.altitude)).asin() / PI;
    assert!(
        (frac - expected).abs() < 0.02,
        "eclipse fraction {frac} vs {expected}"
    );
    let soc = tr.signal("battery.soc").unwrap();
    assert!(
        soc.iter().all(|s| *s > 0.0 && *s <= 1.0 + 1e-9),
        "soc out of range: {:?}",
        soc.iter().cloned().fold(0.0, f64::max)
    );
    // Energy budget over one orbit: battery energy change = ∫(solar − load) dt − losses.
    let e_stored = tr.signal("battery.e_stored").unwrap().last().unwrap();
    let e_loss = tr.signal("battery.e_loss").unwrap().last().unwrap();
    let dt = tr.times[1] - tr.times[0];
    let p_net = tr.signal("battery.power_net").unwrap();
    let supplied: f64 = p_net.iter().skip(1).map(|v| v * dt).sum();
    assert!(
        ((e_stored + e_loss - supplied) / supplied.abs().max(1.0)).abs() < 0.05,
        "energy budget: stored {e_stored} + loss {e_loss} vs supplied {supplied}"
    );
    let power = tr.signal("solar.power").unwrap();
    let pmax = power.iter().cloned().fold(0.0, f64::max);
    // Nominal power at t_ref, times the cold-panel derating bound (T ≥ 150 K → factor ≤ 1.61).
    assert!(
        pmax > 0.0 && pmax <= solar::SOLAR_CONSTANT * np.array_area * np.array_efficiency * 1.61,
        "pmax {pmax}"
    );
    // Power is zero exactly when in eclipse.
    for (e, pw) in ecl.iter().zip(power) {
        assert_eq!(*e > 0.5, *pw == 0.0);
    }
    let t0 = tr.signal("thermal.temperature_0").unwrap();
    assert!(t0.iter().all(|t| *t > 150.0 && *t < 400.0));
    assert!(tr
        .signal("attitude.q_norm")
        .unwrap()
        .iter()
        .all(|n| (n - 1.0).abs() < 1e-6));
}
