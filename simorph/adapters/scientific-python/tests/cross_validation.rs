//! Cross-validation of the wrapped open-source engines against analytic
//! references and against SIMORPH's native engines.  Each test prints the
//! measured numbers (`cargo test -- --nocapture`).

use serde_json::{json, Value};
use simorph_adapter_scientific_python::{check_available, engine, spawn};
use simorph_core::*;
use simorph_simulation::{simulate, DynamicalModel, Params, Solver, SolverOptions};
use std::sync::Arc;

fn get(name: &str) -> Option<Arc<dyn Component>> {
    let spec = engine(name).unwrap();
    if let Err(r) = check_available(spec) {
        eprintln!("SKIPPED {name}: {r}");
        return None;
    }
    Some(spawn(name).unwrap())
}

fn call(c: &Arc<dyn Component>, inputs: Value) -> TaskResult {
    let mut req = TaskRequest::new(c.manifest().id);
    for (k, v) in inputs.as_object().unwrap() {
        req.inputs.insert(k.clone(), v.clone());
    }
    run_component(c.as_ref(), &req, &ExecutionControl::default()).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-300)
}

// ------------------------------------------------------------ native models

struct Ode<F: Fn(f64, &[f64], &mut [f64]) + Send + Sync> {
    n: usize,
    x0: Vec<f64>,
    f: F,
}
impl<F: Fn(f64, &[f64], &mut [f64]) + Send + Sync> DynamicalModel for Ode<F> {
    fn name(&self) -> &str {
        "test.ode"
    }
    fn state_dim(&self) -> usize {
        self.n
    }
    fn state_names(&self) -> Vec<(String, String)> {
        (0..self.n).map(|i| (format!("y{i}"), "1".into())).collect()
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn initial_state(&self, _p: &Params) -> Vec<f64> {
        self.x0.clone()
    }
    fn rhs(&self, t: f64, x: &[f64], dx: &mut [f64], _p: &Params) -> Result<()> {
        (self.f)(t, x, dx);
        Ok(())
    }
}

fn native_end<F: Fn(f64, &[f64], &mut [f64]) + Send + Sync>(
    m: &Ode<F>,
    t_end: f64,
    opts: SolverOptions,
) -> (Vec<f64>, u64) {
    let tr = simulate(
        m,
        &Params::new(),
        None,
        (0.0, t_end),
        &opts,
        &ExecutionControl::default(),
    )
    .unwrap();
    (tr.states.last().unwrap().clone(), tr.stats.rhs_evals)
}

// ------------------------------------------------------------ SciPy solve_ivp

#[test]
fn solve_ivp_decay_matches_analytic_and_native() {
    let Some(c) = get("scipy.integrate.solve_ivp") else {
        return;
    };
    let r = call(
        &c,
        json!({"equations": ["-k*y"], "state_names": ["y"], "y0": [1.0], "t_end": 2.0, "params": {"k": 0.5}, "method": "DOP853", "rtol": 1e-11, "atol": 1e-13}),
    );
    let y = r.get_vec_f64("y_end").unwrap()[0];
    let exact = (-1.0f64).exp();
    let m = Ode {
        n: 1,
        x0: vec![1.0],
        f: |_t: f64, x: &[f64], dx: &mut [f64]| dx[0] = -0.5 * x[0],
    };
    let (yn, _) = native_end(
        &m,
        2.0,
        SolverOptions::new(Solver::Rk45).with_tolerances(1e-11, 1e-13),
    );
    eprintln!(
        "decay: scipy err {:.2e}, native err {:.2e}, |scipy-native| {:.2e}",
        (y - exact).abs(),
        (yn[0] - exact).abs(),
        (y - yn[0]).abs()
    );
    assert!((y - exact).abs() < 1e-10);
    assert!((y - yn[0]).abs() < 1e-9);
    assert_eq!(r.evidence, EvidenceLevel::Simulated);
}

#[test]
fn solve_ivp_van_der_pol_stiff_radau_bdf_vs_native_trapezoidal() {
    let Some(c) = get("scipy.integrate.solve_ivp") else {
        return;
    };
    let base = json!({"equations": ["y2", "mu*(1 - y1^2)*y2 - y1"], "state_names": ["y1", "y2"], "y0": [2.0, 0.0], "params": {"mu": 1000.0}, "rtol": 1e-8, "atol": 1e-10});
    let mut results = vec![];
    for method in ["Radau", "BDF", "LSODA"] {
        let mut inp = base.clone();
        inp["method"] = json!(method);
        inp["t_end"] = json!(500.0);
        let r = call(&c, inp);
        results.push((
            method,
            r.get_vec_f64("y_end").unwrap()[0],
            r.get_f64("nfev").unwrap(),
        ));
    }
    // explicit RK45 on a much shorter horizon to show the stiffness cost
    let mut inp = base.clone();
    inp["method"] = json!("RK45");
    inp["t_end"] = json!(5.0);
    let rk = call(&c, inp);
    let m = Ode {
        n: 2,
        x0: vec![2.0, 0.0],
        f: |_t: f64, x: &[f64], dx: &mut [f64]| {
            dx[0] = x[1];
            dx[1] = 1000.0 * (1.0 - x[0] * x[0]) * x[1] - x[0];
        },
    };
    let (yn, nev) = native_end(
        &m,
        500.0,
        SolverOptions::new(Solver::Trapezoidal).with_dt(0.01),
    );
    for (m_, y1, nfev) in &results {
        eprintln!("VdP mu=1000 t=500 {m_}: y1 = {y1:.8}, nfev = {nfev}, rel diff vs native trapezoidal (dt=0.01) = {:.2e}", rel(*y1, yn[0]));
        assert!(rel(*y1, yn[0]) < 1e-3, "{m_}");
    }
    eprintln!(
        "VdP RK45 over only t=5: nfev = {} (Radau over t=500: {})",
        rk.get_f64("nfev").unwrap(),
        results[0].2
    );
    eprintln!("native trapezoidal rhs evals: {nev}");
    assert!(
        rk.get_f64("nfev").unwrap() > results[0].2,
        "explicit method should already cost more on 1 % of the horizon"
    );
    assert!(rel(results[0].1, results[1].1) < 1e-5);
}

#[test]
fn solve_ivp_robertson_reference_values() {
    let Some(c) = get("scipy.integrate.solve_ivp") else {
        return;
    };
    let r = call(
        &c,
        json!({
        "equations": ["-0.04*y1 + 1e4*y2*y3", "0.04*y1 - 1e4*y2*y3 - 3e7*y2^2", "3e7*y2^2"],
        "state_names": ["y1", "y2", "y3"], "y0": [1.0, 0.0, 0.0], "t_end": 40.0,
        "method": "Radau", "rtol": 1e-8, "atol": 1e-12}),
    );
    let y = r.get_vec_f64("y_end").unwrap();
    // Hairer & Wanner, Solving ODEs II (reference solution at t = 40)
    let refs = [
        0.7158270687193772,
        9.185534764557338e-06,
        0.2841637457458583,
    ];
    for i in 0..3 {
        eprintln!(
            "Robertson y{} = {:.10e} ref {:.10e} rel err {:.2e}",
            i + 1,
            y[i],
            refs[i],
            rel(y[i], refs[i])
        );
        assert!(rel(y[i], refs[i]) < 1e-5);
    }
    assert!(
        (y.iter().sum::<f64>() - 1.0).abs() < 1e-9,
        "mass conservation"
    );
    eprintln!(
        "Robertson Radau nfev = {}, njev = {}",
        r.get_f64("nfev").unwrap(),
        r.get_f64("njev").unwrap()
    );
    // native implicit trapezoidal on the same benchmark
    let m = Ode {
        n: 3,
        x0: vec![1.0, 0.0, 0.0],
        f: |_t: f64, x: &[f64], dx: &mut [f64]| {
            dx[0] = -0.04 * x[0] + 1e4 * x[1] * x[2];
            dx[1] = 0.04 * x[0] - 1e4 * x[1] * x[2] - 3e7 * x[1] * x[1];
            dx[2] = 3e7 * x[1] * x[1];
        },
    };
    let (yn, _) = native_end(
        &m,
        40.0,
        SolverOptions::new(Solver::Trapezoidal).with_dt(1e-3),
    );
    eprintln!(
        "native trapezoidal dt=1e-3: y1 rel err {:.2e}, y3 rel err {:.2e}",
        rel(yn[0], refs[0]),
        rel(yn[2], refs[2])
    );
    assert!(rel(yn[0], refs[0]) < 1e-3);
}

#[test]
fn solve_ivp_terminal_event_free_fall() {
    let Some(c) = get("scipy.integrate.solve_ivp") else {
        return;
    };
    let r = call(
        &c,
        json!({"equations": ["v", "-g"], "state_names": ["h", "v"], "y0": [10.0, 0.0], "t_end": 5.0,
        "params": {"g": 9.81}, "events": ["h"], "rtol": 1e-10, "atol": 1e-12}),
    );
    let te = r.get_f64("event_time").unwrap();
    let exact = (20.0f64 / 9.81).sqrt();
    eprintln!("impact time {te:.12} vs {exact:.12}");
    assert!((te - exact).abs() < 1e-8);
}

#[test]
fn unsafe_expressions_are_rejected_without_evaluation() {
    let Some(c) = get("scipy.integrate.solve_ivp") else {
        return;
    };
    for bad in [
        "__import__('os').system('touch /tmp/simorph_pwned')",
        "y.__class__",
        "open('/etc/passwd')",
    ] {
        let mut req = TaskRequest::new(c.manifest().id);
        for (k, v) in json!({"equations": [bad], "state_names": ["y"], "y0": [1.0], "t_end": 1.0})
            .as_object()
            .unwrap()
        {
            req.inputs.insert(k.clone(), v.clone());
        }
        let e = run_component(c.as_ref(), &req, &ExecutionControl::default());
        assert!(e.is_err(), "{bad} accepted");
    }
    assert!(!std::path::Path::new("/tmp/simorph_pwned").exists());
}

// ------------------------------------------------------------ SciPy optimize

#[test]
fn optimize_rosenbrock_constrained_and_lp() {
    let Some(c) = get("scipy.optimize") else {
        return;
    };
    let r = call(
        &c,
        json!({"kind": "nonlinear", "objective": "(1-x)^2 + 100*(y-x^2)^2", "variables": ["x", "y"], "bounds": [[-2, 2], [-2, 2]], "x0": [-1.2, 1.0], "method": "L-BFGS-B"}),
    );
    let x = r.get_vec_f64("x").unwrap();
    // native Nelder–Mead on the same function
    use simorph_optimization::*;
    let p = benchmarks::rosenbrock_problem(2);
    let nm = nelder_mead::nelder_mead(
        &p,
        Some(&[-1.2, 1.0]),
        &Budget::new(5000),
        &ExecutionControl::default(),
    )
    .unwrap();
    eprintln!("Rosenbrock: scipy L-BFGS-B f = {:.2e} nfev {} ; native NM f = {:.2e} evals {} ; |x_s - x_n| = {:.2e}",
        r.get_f64("fun").unwrap(), r.get_f64("nfev").unwrap(), nm.best_f, nm.evaluations,
        ((x[0] - nm.best_x[0]).powi(2) + (x[1] - nm.best_x[1]).powi(2)).sqrt());
    assert!(r.get_f64("fun").unwrap() < 1e-10);
    assert!((x[0] - 1.0).abs() < 1e-5 && (x[1] - 1.0).abs() < 1e-5);
    assert!((x[0] - nm.best_x[0]).abs() < 1e-3);
    // SciPy documentation SLSQP example: optimum (1.4, 1.7)
    let r = call(
        &c,
        json!({"kind": "nonlinear", "objective": "(x-1)^2 + (y-2.5)^2", "variables": ["x", "y"], "bounds": [[0, 10], [0, 10]], "x0": [2, 0], "method": "SLSQP",
        "constraints": ["x - 2*y + 2", "-x - 2*y + 6", "-x + 2*y + 2"]}),
    );
    let x = r.get_vec_f64("x").unwrap();
    eprintln!(
        "SLSQP: x = {x:?}, violation {:.1e}",
        r.get_f64("max_constraint_violation").unwrap()
    );
    assert!((x[0] - 1.4).abs() < 1e-6 && (x[1] - 1.7).abs() < 1e-6);
    // global: Rastrigin 2-D with differential evolution
    let r = call(
        &c,
        json!({"kind": "nonlinear", "objective": "20 + x^2 - 10*cos(2*pi*x) + y^2 - 10*cos(2*pi*y)", "variables": ["x", "y"], "bounds": [[-5.12, 5.12], [-5.12, 5.12]], "method": "differential_evolution", "seed": 3}),
    );
    eprintln!("Rastrigin DE: f = {:.2e}", r.get_f64("fun").unwrap());
    assert!(r.get_f64("fun").unwrap() < 1e-8);
    // LP (Wyndor Glass): x = (2, 6), z = 36 — also the component's own reference case
    let r = call(
        &c,
        json!({"kind": "linear", "c": [-3, -5], "A_ub": [[1, 0], [0, 2], [3, 2]], "b_ub": [4, 12, 18], "bounds": [[0, null], [0, null]]}),
    );
    let x = r.get_vec_f64("x").unwrap();
    assert!((x[0] - 2.0).abs() < 1e-9 && (x[1] - 6.0).abs() < 1e-9);
    assert!((r.get_f64("fun").unwrap() + 36.0).abs() < 1e-9);
}

// ------------------------------------------------------------ SciPy QMC

#[test]
fn qmc_ishigami_moments_vs_analytic_and_native_sobol() {
    let Some(c) = get("scipy.stats.qmc.moments") else {
        return;
    };
    let pi = std::f64::consts::PI;
    let (a, b) = (7.0, 0.1);
    let mean = a / 2.0;
    let var = a * a / 8.0 + b * pi.powi(4) / 5.0 + b * b * pi.powi(8) / 18.0 + 0.5;
    let r = call(
        &c,
        json!({"function": "sin(x1) + 7*sin(x2)^2 + 0.1*x3^4*sin(x1)", "variables": ["x1", "x2", "x3"],
        "bounds": [[-pi, pi], [-pi, pi], [-pi, pi]], "n": 4096, "sampler": "sobol", "seed": 1}),
    );
    let (m, v, se) = (
        r.get_f64("mean").unwrap(),
        r.get_f64("variance").unwrap(),
        r.get_f64("std_error_mean").unwrap(),
    );
    // native (unscrambled) Sobol from simorph-uncertainty
    let pts = simorph_uncertainty::sobol(4096, 3, true);
    let f = |u: &[f64]| {
        let x: Vec<f64> = u.iter().map(|v| -pi + 2.0 * pi * v).collect();
        x[0].sin() + 7.0 * x[1].sin().powi(2) + 0.1 * x[2].powi(4) * x[0].sin()
    };
    let vals: Vec<f64> = pts.iter().map(|p| f(p)).collect();
    let mn = vals.iter().sum::<f64>() / vals.len() as f64;
    eprintln!("Ishigami mean: analytic {mean}, scipy {m:.6} (se {se:.1e}), native sobol {mn:.6}; variance analytic {var:.4}, scipy {v:.4}; discrepancy {:.2e}", r.get_f64("discrepancy").unwrap());
    assert!((m - mean).abs() < 4.0 * se + 1e-3);
    assert!((mn - mean).abs() < 5e-3);
    assert!(rel(v, var) < 0.01);
}

// ------------------------------------------------------------ SymPy

#[test]
fn sympy_dsolve_damped_oscillator_vs_analytic_and_native() {
    let Some(c) = get("sympy.dsolve") else { return };
    let r = call(
        &c,
        json!({"lhs": "y2 + 2*y1 + 5*y", "rhs": "0", "order": 2, "ics": [[0, 0, 1], [1, 0, 0]], "eval_times": [0.5, 2.0]}),
    );
    assert_eq!(r.get_f64("verified").unwrap(), 1.0);
    let v = r.get_vec_f64("values").unwrap();
    let exact = |t: f64| (-t).exp() * ((2.0 * t).cos() + 0.5 * (2.0 * t).sin());
    let m = Ode {
        n: 2,
        x0: vec![1.0, 0.0],
        f: |_t: f64, x: &[f64], dx: &mut [f64]| {
            dx[0] = x[1];
            dx[1] = -2.0 * x[1] - 5.0 * x[0];
        },
    };
    let (yn, _) = native_end(
        &m,
        2.0,
        SolverOptions::new(Solver::Rk45).with_tolerances(1e-12, 1e-14),
    );
    eprintln!(
        "dsolve: {} ; y(2) sympy {:.15} exact {:.15} native {:.15}",
        r.outputs["solution"],
        v[1],
        exact(2.0),
        yn[0]
    );
    assert!((v[0] - exact(0.5)).abs() < 1e-14 && (v[1] - exact(2.0)).abs() < 1e-14);
    assert!((v[1] - yn[0]).abs() < 1e-9);
    // first order with forcing and a parameter
    let r = call(
        &c,
        json!({"lhs": "y1 + k*y", "rhs": "sin(t)", "order": 1, "ics": [[0, 0, 0]], "eval_times": [3.0], "params": {"k": 0.5}}),
    );
    assert_eq!(r.get_f64("verified").unwrap(), 1.0);
    let k = 0.5f64;
    let t = 3.0f64;
    let ex = (k * t.sin() - t.cos() + (-k * t).exp()) / (1.0 + k * k);
    eprintln!(
        "forced first order: {} ; y(3) {:.15} vs {:.15}",
        r.outputs["solution"],
        r.get_vec_f64("values").unwrap()[0],
        ex
    );
    assert!((r.get_vec_f64("values").unwrap()[0] - ex).abs() < 1e-13);
}

// ------------------------------------------------------------ MuJoCo

/// Complete elliptic integral of the first kind K(k) via the AGM.
fn ellip_k(k: f64) -> f64 {
    let (mut a, mut b) = (1.0f64, (1.0 - k * k).sqrt());
    for _ in 0..30 {
        let (an, bn) = (0.5 * (a + b), (a * b).sqrt());
        a = an;
        b = bn;
    }
    std::f64::consts::PI / (2.0 * a)
}

#[test]
fn mujoco_pendulum_periods_vs_analytic_and_native() {
    let Some(c) = get("mujoco.simulation") else {
        return;
    };
    let (l, g) = (1.0f64, 9.81f64);
    let small = call(
        &c,
        json!({"model": "pendulum", "length": l, "duration": 5.0, "qpos0": [0.01]}),
    );
    let t_small = small.get_f64("period").unwrap();
    let t0 = 2.0 * std::f64::consts::PI * (l / g).sqrt();
    eprintln!("MuJoCo pendulum θ0=0.01: T = {t_small:.8} vs 2π√(L/g) = {t0:.8} (rel {:.1e}); {:.0} steps/s", rel(t_small, t0), small.metrics["steps_per_second"]);
    assert!(rel(t_small, t0) < 1e-4);
    let th = 2.5f64;
    let big = call(
        &c,
        json!({"model": "pendulum", "length": l, "duration": 10.0, "qpos0": [th]}),
    );
    let t_big = big.get_f64("period").unwrap();
    let exact = 4.0 * (l / g).sqrt() * ellip_k((th / 2.0).sin());
    // native nonlinear pendulum, period from downward zero crossings
    struct Pend;
    impl DynamicalModel for Pend {
        fn name(&self) -> &str {
            "pend"
        }
        fn state_dim(&self) -> usize {
            2
        }
        fn state_names(&self) -> Vec<(String, String)> {
            vec![("th".into(), "rad".into()), ("w".into(), "rad/s".into())]
        }
        fn parameter_names(&self) -> Vec<(String, String)> {
            vec![]
        }
        fn initial_state(&self, _p: &Params) -> Vec<f64> {
            vec![2.5, 0.0]
        }
        fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], _p: &Params) -> Result<()> {
            dx[0] = x[1];
            dx[1] = -9.81 * x[0].sin();
            Ok(())
        }
    }
    let tr = simulate(
        &Pend,
        &Params::new(),
        None,
        (0.0, 10.0),
        &SolverOptions::new(Solver::Rk4).with_dt(1e-4),
        &ExecutionControl::default(),
    )
    .unwrap();
    let mut cr = vec![];
    for i in 1..tr.times.len() {
        let (a, b) = (tr.states[i - 1][0], tr.states[i][0]);
        if a > 0.0 && b <= 0.0 {
            cr.push(tr.times[i - 1] + (tr.times[i] - tr.times[i - 1]) * a / (a - b));
        }
    }
    let t_native = cr[1] - cr[0];
    eprintln!("θ0=2.5: MuJoCo T = {t_big:.8}, native RK4 T = {t_native:.8}, exact 4√(L/g)K = {exact:.8}; rel errs {:.1e} / {:.1e}; energy drift {:.1e}",
        rel(t_big, exact), rel(t_native, exact), big.get_f64("energy_drift").unwrap());
    assert!(rel(t_big, exact) < 1e-4);
    assert!(rel(t_native, exact) < 1e-4);
    assert!(big.get_f64("energy_drift").unwrap().abs() < 1e-8);
}

#[test]
fn mujoco_free_fall_cartpole_energy_and_mjcf_guard() {
    let Some(c) = get("mujoco.simulation") else {
        return;
    };
    let r = call(&c, json!({"model": "free_fall", "duration": 1.0}));
    let z = r.get_vec_f64("qpos_end").unwrap()[2];
    eprintln!("free fall z(1) = {z:.12} vs {:.12}", 10.0 - 0.5 * 9.81);
    assert!((z - (10.0 - 0.5 * 9.81)).abs() < 1e-6);
    let cp = call(
        &c,
        json!({"model": "cartpole", "duration": 3.0, "qpos0": [0.0, 0.3]}),
    );
    eprintln!(
        "cart-pole passive: energy drift {:.2e}, {:.0} steps/s",
        cp.get_f64("energy_drift").unwrap(),
        cp.metrics["steps_per_second"]
    );
    assert!(cp.get_f64("energy_drift").unwrap().abs() < 1e-8);
    let mut req = TaskRequest::new(c.manifest().id);
    req.inputs.insert("duration".into(), json!(1.0));
    req.inputs.insert(
        "mjcf".into(),
        json!("<mujoco><include file=\"/etc/passwd\"/></mujoco>"),
    );
    assert!(run_component(c.as_ref(), &req, &ExecutionControl::default()).is_err());
}

// ------------------------------------------------------------ Gymnasium

/// Independent re-implementation of Gymnasium CartPole-v1 dynamics (Euler).
fn cartpole_step(s: [f64; 4], action: i64) -> [f64; 4] {
    let (g, mc, mp, l, fmag, tau) = (9.8, 1.0, 0.1, 0.5, 10.0, 0.02);
    let total = mc + mp;
    let pml = mp * l;
    let [x, xd, th, thd] = s;
    let force = if action == 1 { fmag } else { -fmag };
    let (ct, st) = (th.cos(), th.sin());
    let temp = (force + pml * thd * thd * st) / total;
    let thacc = (g * st - ct * temp) / (l * (4.0 / 3.0 - mp * ct * ct / total));
    let xacc = temp - pml * thacc * ct / total;
    [
        x + tau * xd,
        xd + tau * xacc,
        th + tau * thd,
        thd + tau * thacc,
    ]
}

#[test]
fn gymnasium_cartpole_deterministic_and_matches_independent_dynamics() {
    let Some(c) = get("gymnasium.episode") else {
        return;
    };
    let actions = json!([1, 0, 1, 1, 0, 0, 1, 0, 1, 1]);
    let a = call(
        &c,
        json!({"env_id": "CartPole-v1", "seed": 0, "actions": actions}),
    );
    let b = call(
        &c,
        json!({"env_id": "CartPole-v1", "seed": 0, "actions": actions}),
    );
    assert_eq!(a.outputs["observations"], b.outputs["observations"]);
    let init = a.get_vec_f64("initial_observation").unwrap();
    let mut s = [init[0], init[1], init[2], init[3]];
    let obs = a.outputs["observations"].as_array().unwrap();
    let mut worst = 0.0f64;
    for (k, act) in actions.as_array().unwrap().iter().enumerate() {
        s = cartpole_step(s, act.as_i64().unwrap());
        let o: Vec<f64> = obs[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        for i in 0..4 {
            worst = worst.max((o[i] - s[i]).abs());
        }
    }
    eprintln!(
        "CartPole: max |gym - independent| over 10 steps = {worst:.2e} (float32 observations)"
    );
    assert!(worst < 1e-5);
    // a linear policy balances much longer than a constant push
    let good = call(
        &c,
        json!({"env_id": "CartPole-v1", "seed": 1, "policy": {"weights": [0.0, 0.0, 1.0, 1.0], "bias": 0.0}}),
    );
    let bad = call(
        &c,
        json!({"env_id": "CartPole-v1", "seed": 1, "policy": {"weights": [0.0, 0.0, 0.0, 0.0], "bias": 1.0}}),
    );
    eprintln!(
        "CartPole return: linear policy {}, constant push {}",
        good.get_f64("total_reward").unwrap(),
        bad.get_f64("total_reward").unwrap()
    );
    assert!(good.get_f64("total_reward").unwrap() > 5.0 * bad.get_f64("total_reward").unwrap());
}

// ------------------------------------------------------------ PyTorch

#[test]
fn torch_mlp_learns_sine_deterministically() {
    let Some(c) = get("pytorch.mlp_regression") else {
        return;
    };
    let pi = std::f64::consts::PI;
    let xs = |n: usize, off: f64| -> Vec<f64> {
        (0..n)
            .map(|i| -pi + 2.0 * pi * (i as f64 + off) / n as f64)
            .collect()
    };
    let (xtr, xte) = (xs(64, 0.0), xs(64, 0.5));
    let col = |v: &Vec<f64>| v.iter().map(|x| json!([x])).collect::<Vec<_>>();
    let ys = |v: &Vec<f64>| v.iter().map(|x| x.sin()).collect::<Vec<_>>();
    let inp = json!({"train_x": col(&xtr), "train_y": ys(&xtr), "test_x": col(&xte), "test_y": ys(&xte), "hidden": [32, 32], "epochs": 1500, "lr": 0.01, "seed": 7});
    let a = call(&c, inp.clone());
    let b = call(&c, inp);
    let te = a.get_f64("test_mse").unwrap();
    let baseline = ys(&xte).iter().map(|y| y * y).sum::<f64>() / 64.0;
    eprintln!(
        "torch MLP sin: test MSE {te:.2e} vs mean-predictor {baseline:.3}; params {}; {:.2} s",
        a.get_f64("n_parameters").unwrap(),
        a.metrics["train_wall_s"]
    );
    assert!(te < 1e-3);
    assert_eq!(a.outputs["weights_sha256"], b.outputs["weights_sha256"]);
}
