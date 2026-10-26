//! Verification of the integrators against analytic solutions.

use indexmap::IndexMap;
use simorph_core::{Component, ExecutionControl, TaskRequest};
use simorph_simulation::model::{param, EventAction, EventDirection};
use simorph_simulation::{simulate, DynamicalModel, OdeComponent, Params, Solver, SolverOptions};
use std::sync::Arc;
use std::time::Duration;

fn p(pairs: &[(&str, f64)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// dx/dt = -k x, x(0)=1 -> exp(-k t)
struct Decay;
impl DynamicalModel for Decay {
    fn name(&self) -> &str {
        "test.decay"
    }
    fn state_dim(&self) -> usize {
        1
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "1".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![("k".into(), "1/s".into())]
    }
    fn initial_state(&self, _: &Params) -> Vec<f64> {
        vec![1.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> simorph_core::Result<()> {
        dx[0] = -param(params, "k")? * x[0];
        Ok(())
    }
    fn outputs(&self, _t: f64, x: &[f64], _p: &Params) -> IndexMap<String, f64> {
        IndexMap::from([("half".to_string(), 0.5 * x[0])])
    }
    fn output_names(&self) -> Vec<(String, String)> {
        vec![("half".into(), "1".into())]
    }
}

/// Harmonic oscillator x'' = -w^2 x, x(0)=1, v(0)=0.
struct Oscillator;
impl DynamicalModel for Oscillator {
    fn name(&self) -> &str {
        "test.oscillator"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "m".into()), ("v".into(), "m/s".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![("w".into(), "rad/s".into())]
    }
    fn initial_state(&self, _: &Params) -> Vec<f64> {
        vec![1.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> simorph_core::Result<()> {
        let w = param(params, "w")?;
        dx[0] = x[1];
        dx[1] = -w * w * x[0];
        Ok(())
    }
}

/// Van der Pol oscillator with large mu (stiff).
struct VanDerPol;
impl DynamicalModel for VanDerPol {
    fn name(&self) -> &str {
        "test.vdp"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "1".into()), ("v".into(), "1".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![("mu".into(), "1".into())]
    }
    fn initial_state(&self, _: &Params) -> Vec<f64> {
        vec![2.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> simorph_core::Result<()> {
        let mu = param(params, "mu")?;
        dx[0] = x[1];
        dx[1] = mu * (1.0 - x[0] * x[0]) * x[1] - x[0];
        Ok(())
    }
}

/// Bouncing ball: y'' = -g, bounce with restitution e when y crosses 0 downward.
struct Ball;
impl DynamicalModel for Ball {
    fn name(&self) -> &str {
        "test.ball"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("y".into(), "m".into()), ("v".into(), "m/s".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![("g".into(), "m/s^2".into()), ("e".into(), "1".into())]
    }
    fn initial_state(&self, _: &Params) -> Vec<f64> {
        vec![10.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> simorph_core::Result<()> {
        dx[0] = x[1];
        dx[1] = -param(params, "g")?;
        Ok(())
    }
    fn num_events(&self) -> usize {
        1
    }
    fn event_value(&self, _i: usize, _t: f64, x: &[f64], _p: &Params) -> f64 {
        x[0]
    }
    fn event_direction(&self, _i: usize) -> EventDirection {
        EventDirection::Falling
    }
    fn handle_event(&self, _i: usize, _t: f64, x: &mut [f64], params: &Params) -> EventAction {
        x[0] = 0.0;
        x[1] *= -param(params, "e").unwrap();
        EventAction::Continue
    }
}

fn ctl() -> ExecutionControl {
    ExecutionControl::default()
}

#[test]
fn exponential_decay_all_solvers() {
    let k: f64 = 1.3;
    let t_end: f64 = 3.0;
    let exact = (-k * t_end).exp();
    let cases = [
        (Solver::Euler, 1e-4, 2e-4),
        (Solver::Rk4, 1e-2, 1e-9),
        (Solver::Rk45, 1e-2, 1e-6),
        (Solver::BackwardEuler, 1e-4, 2e-4),
        (Solver::Trapezoidal, 1e-3, 1e-6),
    ];
    for (solver, dt, tol) in cases {
        let opts = SolverOptions::new(solver)
            .with_dt(dt)
            .with_tolerances(1e-9, 1e-12);
        let tr = simulate(&Decay, &p(&[("k", k)]), None, (0.0, t_end), &opts, &ctl()).unwrap();
        let err = (tr.final_state()[0] - exact).abs();
        assert!(err < tol, "{solver:?}: err={err:e} tol={tol:e}");
        assert!((tr.final_time() - t_end).abs() < 1e-12);
        assert_eq!(tr.outputs["half"].len(), tr.len());
    }
}

#[test]
fn harmonic_oscillator_energy_and_phase() {
    let w = 2.0;
    let t_end = 10.0 * std::f64::consts::PI; // 10 periods
    let opts = SolverOptions::new(Solver::Rk45).with_tolerances(1e-10, 1e-12);
    let tr = simulate(
        &Oscillator,
        &p(&[("w", w)]),
        None,
        (0.0, t_end),
        &opts,
        &ctl(),
    )
    .unwrap();
    for (t, s) in tr.times.iter().zip(&tr.states) {
        let e = 0.5 * s[1] * s[1] + 0.5 * w * w * s[0] * s[0];
        assert!((e - 0.5 * w * w).abs() < 1e-7, "energy drift {e}");
        assert!((s[0] - (w * t).cos()).abs() < 1e-6, "phase error at t={t}");
    }
    assert!(tr.stats.max_error_estimate <= 1.0);
    assert!(tr.stats.steps > 100);
}

fn measured_order(solver: Solver) -> f64 {
    // Global error vs step size on the oscillator; order = log2(e(h)/e(h/2)).
    let w: f64 = 1.0;
    let t_end: f64 = 2.0;
    let exact = (w * t_end).cos();
    let errs: Vec<f64> = [0.1, 0.05, 0.025]
        .iter()
        .map(|&h| {
            // Huge tolerances: every step is accepted so RK45 runs at the fixed step h.
            let opts = SolverOptions::new(solver)
                .with_dt(h)
                .with_dt_max(h)
                .with_tolerances(1e9, 1e9);
            let tr = simulate(
                &Oscillator,
                &p(&[("w", w)]),
                None,
                (0.0, t_end),
                &opts,
                &ctl(),
            )
            .unwrap();
            (tr.final_state()[0] - exact).abs()
        })
        .collect();
    let o1 = (errs[0] / errs[1]).log2();
    let o2 = (errs[1] / errs[2]).log2();
    0.5 * (o1 + o2)
}

#[test]
fn convergence_orders() {
    let o_euler = measured_order(Solver::Euler);
    let o_rk4 = measured_order(Solver::Rk4);
    let o_rk45 = measured_order(Solver::Rk45);
    let o_be = measured_order(Solver::BackwardEuler);
    let o_tr = measured_order(Solver::Trapezoidal);
    assert!((o_euler - 1.0).abs() < 0.2, "euler order {o_euler}");
    assert!((o_rk4 - 4.0).abs() < 0.3, "rk4 order {o_rk4}");
    assert!(o_rk45 > 4.5, "rk45 order {o_rk45}");
    assert!((o_be - 1.0).abs() < 0.2, "backward euler order {o_be}");
    assert!((o_tr - 2.0).abs() < 0.2, "trapezoidal order {o_tr}");
}

#[test]
fn stiff_van_der_pol_implicit_is_stable() {
    let params = p(&[("mu", 1000.0)]);
    // Explicit Euler with this step is unstable (blows up or errors).
    let bad = simulate(
        &VanDerPol,
        &params,
        None,
        (0.0, 5.0),
        &SolverOptions::new(Solver::Euler).with_dt(0.01),
        &ctl(),
    );
    let exploded = match bad {
        Err(_) => true,
        Ok(t) => t.final_state().iter().any(|v| v.abs() > 1e3),
    };
    assert!(exploded, "explicit Euler should fail on stiff VdP");
    for solver in [Solver::BackwardEuler, Solver::Trapezoidal] {
        let tr = simulate(
            &VanDerPol,
            &params,
            None,
            (0.0, 5.0),
            &SolverOptions::new(solver).with_dt(0.01),
            &ctl(),
        )
        .unwrap();
        // On the slow manifold x stays bounded in [-2.1, 2.1].
        assert!(
            tr.states.iter().all(|s| s[0].abs() < 2.1),
            "{solver:?} unstable"
        );
        assert!(tr.stats.newton_iters > 0 && tr.stats.jacobian_evals > 0);
        // x decays slowly from 2 along the manifold: x(5) in (1.5, 2.0).
        let xf = tr.final_state()[0];
        assert!(xf > 1.5 && xf < 2.0, "{solver:?} x(5)={xf}");
    }
}

#[test]
fn bouncing_ball_event_times() {
    let (g, e, y0) = (9.81, 0.8, 10.0);
    let params = p(&[("g", g), ("e", e)]);
    let opts = SolverOptions::new(Solver::Rk4).with_dt(0.01);
    let tr = simulate(&Ball, &params, None, (0.0, 6.0), &opts, &ctl()).unwrap();
    // Analytic: t1 = sqrt(2 y0/g); flight k lasts 2 v_k / g with v_k = e^k v_1.
    let v1 = (2.0 * g * y0).sqrt();
    let mut t_expected = vec![(2.0 * y0 / g).sqrt()];
    let mut v = v1;
    while *t_expected.last().unwrap() < 6.0 {
        v *= e;
        let next = t_expected.last().unwrap() + 2.0 * v / g;
        t_expected.push(next);
    }
    t_expected.retain(|t| *t <= 6.0);
    assert_eq!(tr.events.len(), t_expected.len(), "events {:?}", tr.events);
    for (ev, te) in tr.events.iter().zip(&t_expected) {
        assert!(
            (ev.time - te).abs() < 1e-6,
            "event at {} expected {te}",
            ev.time
        );
    }
    assert!(tr.states.iter().all(|s| s[0] >= -1e-9));
}

#[test]
fn cancellation_and_deadline_stop_long_runs() {
    let ctl = ExecutionControl::default();
    ctl.cancel.cancel();
    let r = simulate(
        &Decay,
        &p(&[("k", 1.0)]),
        None,
        (0.0, 1e9),
        &SolverOptions::new(Solver::Euler).with_dt(1e-3),
        &ctl,
    );
    assert!(matches!(r, Err(simorph_core::SimorphError::Cancelled)));
    let ctl = ExecutionControl::with_timeout(Duration::from_millis(50));
    let r = simulate(
        &Decay,
        &p(&[("k", 1.0)]),
        None,
        (0.0, 1e12),
        &SolverOptions::new(Solver::Euler).with_dt(1e-6),
        &ctl,
    );
    assert!(matches!(r, Err(simorph_core::SimorphError::Timeout(_))));
}

#[test]
fn ode_component_contract() {
    let c = OdeComponent::new(Arc::new(Decay), "0.1.0", "test");
    let m = c.manifest();
    m.validate_structure().unwrap();
    assert!(m
        .input_schema
        .iter()
        .any(|p| p.name == "k" && p.unit == "1/s"));
    assert!(m.output_schema.iter().any(|p| p.name == "final_x"));
    let req = TaskRequest::new(m.id).input("k", 2.0).input("t_end", 1.0);
    let res = simorph_core::run_component(&c, &req, &ctl()).unwrap();
    assert!((res.get_f64("final_x").unwrap() - (-2.0f64).exp()).abs() < 1e-6);
    assert_eq!(res.evidence, simorph_core::EvidenceLevel::Simulated);
    assert!(res.metrics["steps"] > 0.0);
    let traj = &res.outputs["trajectory"];
    assert_eq!(
        traj["times"].as_array().unwrap().len(),
        traj["states"]["x"].as_array().unwrap().len()
    );
    // Missing input is rejected by the contract, not by a panic.
    let bad = TaskRequest::new(m.id).input("t_end", 1.0);
    assert!(simorph_core::run_component(&c, &bad, &ctl()).is_err());
    // Config override and reduced fidelity.
    let req2 = TaskRequest::new(m.id)
        .input("k", 2.0)
        .input("t_end", 1.0)
        .fidelity(0.5)
        .config(serde_json::json!({"solver": "rk4", "dt": 0.05, "x0": [3.0], "max_points": 5}));
    let res2 = c.execute(&req2, &ctl()).unwrap();
    // Fidelity 0.5 enlarges the RK4 step (~x3.2): accuracy is reduced but bounded.
    assert!((res2.get_f64("final_x").unwrap() - 3.0 * (-2.0f64).exp()).abs() < 1e-3);
    assert_eq!(
        res2.outputs["trajectory"]["times"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(res2.warnings.iter().any(|w| w.contains("fidelity")));
}
