//! Target systems: a validated simulator, its input domain, feature
//! transforms, physical bounds and protected regimes.

use indexmap::IndexMap;
use serde_json::json;
use simorph_core::*;
use simorph_learning::surrogate::Transform;
use simorph_simulation::{
    simulate, DynamicalModel, EventAction, EventDirection, Params, Solver, SolverOptions,
};
use std::sync::Arc;

/// One input dimension of the surrogate.
#[derive(Debug, Clone)]
pub struct InputDim {
    pub name: String,
    pub unit: String,
    pub low: f64,
    pub high: f64,
    pub transform: Transform,
}

impl InputDim {
    pub fn t_low(&self) -> f64 {
        self.transform.forward(self.low).expect("valid bound")
    }
    pub fn t_high(&self) -> f64 {
        self.transform.forward(self.high).expect("valid bound")
    }
}

/// A simulator whose surrogate SIMORPH tries to improve.
pub struct TargetSystem {
    pub name: String,
    pub component: Arc<dyn Component>,
    pub inputs: Vec<InputDim>,
    /// Inputs held constant (component port units).
    pub fixed: IndexMap<String, f64>,
    pub output: String,
    pub output_unit: String,
    pub output_transform: Transform,
    /// Physical bounds every validated sample must satisfy (inputs → output).
    pub bounds: fn(&IndexMap<String, f64>, f64) -> std::result::Result<(), String>,
    /// Protected regime: (name, membership predicate on the inputs).
    pub protected_regime: (String, fn(&IndexMap<String, f64>) -> bool),
    /// Diagnostic regime label for the error map.
    pub regime_label: fn(&IndexMap<String, f64>) -> String,
    /// Monotonicity requirement: (input index, increasing).
    pub monotone_in: Option<(usize, bool)>,
}

impl TargetSystem {
    pub fn input_map(&self, x: &[f64]) -> IndexMap<String, f64> {
        self.inputs
            .iter()
            .zip(x)
            .map(|(d, v)| (d.name.clone(), *v))
            .collect()
    }
    /// Network features from physical inputs.
    pub fn features(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .zip(&self.inputs)
            .map(|(v, d)| d.transform.forward(*v).expect("inside domain"))
            .collect()
    }
    /// Run the simulator directly (used only to build references).
    pub fn simulate(&self, x: &[f64], ctl: &ExecutionControl) -> Result<f64> {
        let mut req = TaskRequest::new(self.component.manifest().id);
        for (k, v) in &self.fixed {
            req.inputs.insert(k.clone(), json!(v));
        }
        for (d, v) in self.inputs.iter().zip(x) {
            req.inputs.insert(d.name.clone(), json!(v));
        }
        run_component(self.component.as_ref(), &req, ctl)?.get_f64(&self.output)
    }
}

// ------------------------------------------------------------ pipe flow

/// Darcy–Weisbach pipe flow (plugins/engineering), water at 20 °C, 10 m pipe.
/// Reynolds number spans ~50 to ~2·10⁵: laminar, transitional and turbulent
/// regimes — a discontinuous-slope target that is hard for a smooth surrogate.
pub fn pipe_flow_target() -> TargetSystem {
    let mut fixed = IndexMap::new();
    fixed.insert("length".into(), 10.0);
    fixed.insert("density".into(), 1000.0);
    fixed.insert("viscosity".into(), 1e-3);
    fn reynolds(x: &IndexMap<String, f64>) -> f64 {
        1000.0 * x["velocity"] * x["diameter"] / 1e-3
    }
    TargetSystem {
        name: "pipe_flow".into(),
        component: Arc::new(simorph_plugin_engineering::pipe_flow::PipeFlow::new()),
        inputs: vec![
            InputDim {
                name: "diameter".into(),
                unit: "m".into(),
                low: 0.005,
                high: 0.1,
                transform: Transform::Log10,
            },
            InputDim {
                name: "velocity".into(),
                unit: "m/s".into(),
                low: 0.01,
                high: 2.0,
                transform: Transform::Log10,
            },
            InputDim {
                name: "roughness".into(),
                unit: "m".into(),
                low: 1e-6,
                high: 1e-4,
                transform: Transform::Log10,
            },
        ],
        fixed,
        output: "pressure_drop".into(),
        output_unit: "Pa".into(),
        output_transform: Transform::Log10,
        bounds: |x, dp| {
            if !dp.is_finite() || dp <= 0.0 {
                return Err(format!("pressure drop {dp} must be finite and positive"));
            }
            // Laminar lower bound: Poiseuille (f = 64/Re) is the minimum friction for Re < 2300.
            let re = 1000.0 * x["velocity"] * x["diameter"] / 1e-3;
            if re < 2300.0 {
                let dp_lam = 32.0 * 1e-3 * 10.0 * x["velocity"] / (x["diameter"] * x["diameter"]);
                if (dp - dp_lam).abs() > 1e-9 * dp_lam {
                    return Err(format!(
                        "laminar sample deviates from Hagen–Poiseuille: {dp} vs {dp_lam}"
                    ));
                }
            }
            Ok(())
        },
        protected_regime: ("laminaire (Re < 2000)".into(), |x| reynolds(x) < 2000.0),
        regime_label: |x| {
            let re = reynolds(x);
            if re < 2300.0 {
                "laminaire".into()
            } else if re <= 4000.0 {
                "transition".into()
            } else {
                "turbulent".into()
            }
        },
        monotone_in: Some((1, true)),
    }
}

// ------------------------------------------------------------ pendulum period

/// Complete elliptic integral of the first kind K(m), m = k², by the
/// arithmetic–geometric mean: K = π / (2 AGM(1, √(1−m))).
pub fn elliptic_k(m: f64) -> f64 {
    let (mut a, mut b) = (1.0f64, (1.0 - m).sqrt());
    for _ in 0..60 {
        let (an, bn) = ((a + b) / 2.0, (a * b).sqrt());
        a = an;
        b = bn;
        if (a - b).abs() < 1e-16 * a {
            break;
        }
    }
    std::f64::consts::PI / (2.0 * a)
}

/// Exact period of the nonlinear pendulum: T = 4 √(L/g) K(sin²(θ0/2)).
pub fn pendulum_exact_period(theta0: f64, length: f64, g: f64) -> f64 {
    4.0 * (length / g).sqrt() * elliptic_k((theta0 / 2.0).sin().powi(2))
}

struct PendulumOde;

impl DynamicalModel for PendulumOde {
    fn name(&self) -> &str {
        "pendulum"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![
            ("theta".into(), "rad".into()),
            ("omega".into(), "rad/s".into()),
        ]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("theta0".into(), "rad".into()),
            ("length".into(), "m".into()),
            ("g".into(), "m/s^2".into()),
        ]
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        vec![p["theta0"], 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        dx[0] = x[1];
        dx[1] = -(p["g"] / p["length"]) * x[0].sin();
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
    fn handle_event(&self, _i: usize, _t: f64, _x: &mut [f64], _p: &Params) -> EventAction {
        EventAction::Terminate
    }
}

/// Simulator: period of a released pendulum, by RK45 integration of
/// θ'' = −(g/L) sin θ until the first zero crossing of θ (quarter period).
pub struct PendulumPeriod {
    m: ComponentManifest,
}

impl PendulumPeriod {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process(
            "physics.pendulum.period",
            "0.1.0",
            "simorph-svc-improvement-loop",
        )
        .with_capabilities(vec![Capability::OdeIntegration])
        .with_domains(&["physics", "mechanics"])
        .with_inputs(vec![
            Port::scalar("theta0", "rad").with_range(1e-3, 3.13),
            Port::scalar("length", "m").with_range(1e-3, 1e3),
        ])
        .with_outputs(vec![Port::scalar("period", "s")])
        .with_assumptions(&[
            "rigid massless rod, point mass, no friction, released at rest",
            "g = 9.81 m/s²",
            "RK45 with rtol 1e-11; event location by bisection",
        ])
        .with_validation(ValidationProtocol {
            reference_cases: vec![ReferenceCase {
                name: "θ0 = 2 rad, L = 1 m".into(),
                inputs: json!({"theta0": 2.0, "length": 1.0}),
                expected_outputs: json!({"period": pendulum_exact_period(2.0, 1.0, 9.81)}),
                rtol: 1e-7,
                atol: 0.0,
                source: "closed form 4√(L/g)·K(sin²(θ0/2)), K by arithmetic–geometric mean".into(),
            }],
            invariants: vec!["period > 0".into()],
            convergence_order: None,
        });
        Self { m }
    }
}

impl Default for PendulumPeriod {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for PendulumPeriod {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let mut p = Params::new();
        p.insert("theta0".into(), req.get_f64("theta0")?);
        p.insert("length".into(), req.get_f64("length")?);
        p.insert("g".into(), 9.81);
        let t_max = 40.0 * (p["length"] / 9.81).sqrt() * 10.0;
        let opts = SolverOptions::new(Solver::Rk45).with_tolerances(1e-11, 1e-13);
        let tr = simulate(&PendulumOde, &p, None, (0.0, t_max), &opts, ctl)?;
        if !tr.terminated_by_event {
            return Err(SimorphError::Numerical(
                "pendulum did not cross the vertical within the horizon".into(),
            ));
        }
        let t1 = tr
            .events
            .first()
            .map(|e| e.time)
            .ok_or_else(|| SimorphError::Numerical("no event".into()))?;
        Ok(TaskResult::new(req, EvidenceLevel::Simulated)
            .output("period", 4.0 * t1)
            .metric("rhs_evals", tr.stats.rhs_evals as f64))
    }
}

/// Pendulum period over amplitudes 0.1–3.0 rad (period diverges as θ0 → π)
/// and lengths 0.25–4 m.
pub fn pendulum_target() -> TargetSystem {
    TargetSystem {
        name: "pendulum_period".into(),
        component: Arc::new(PendulumPeriod::new()),
        inputs: vec![
            InputDim {
                name: "theta0".into(),
                unit: "rad".into(),
                low: 0.1,
                high: 3.0,
                transform: Transform::Identity,
            },
            InputDim {
                name: "length".into(),
                unit: "m".into(),
                low: 0.25,
                high: 4.0,
                transform: Transform::Log10,
            },
        ],
        fixed: IndexMap::new(),
        output: "period".into(),
        output_unit: "s".into(),
        output_transform: Transform::Log10,
        bounds: |x, t| {
            let t0 = 2.0 * std::f64::consts::PI * (x["length"] / 9.81).sqrt();
            if !t.is_finite() || t < t0 * (1.0 - 1e-9) {
                return Err(format!("period {t} below the small-angle limit {t0} (impossible for a released pendulum)"));
            }
            Ok(())
        },
        protected_regime: ("petites amplitudes (θ0 < 1 rad)".into(), |x| {
            x["theta0"] < 1.0
        }),
        regime_label: |x| {
            let a = x["theta0"];
            if a < 1.0 {
                "θ0 < 1".into()
            } else if a < 2.2 {
                "1 ≤ θ0 < 2.2".into()
            } else {
                "θ0 ≥ 2.2 (proche de π)".into()
            }
        },
        monotone_in: Some((0, true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pendulum_simulator_matches_elliptic_integral() {
        // K(0) = π/2 and the small-amplitude series 2π√(L/g)(1 + θ²/16)
        assert!((elliptic_k(0.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        // K(0.5) = 1.854074677301372 (Abramowitz & Stegun table 17.1)
        assert!((elliptic_k(0.5) - 1.854_074_677_301_372).abs() < 1e-12);
        let t = pendulum_target();
        let ctl = ExecutionControl::default();
        for (th, l) in [(0.1, 1.0), (1.0, 0.5), (2.5, 2.0), (3.0, 0.25)] {
            let sim = t.simulate(&[th, l], &ctl).unwrap();
            let exact = pendulum_exact_period(th, l, 9.81);
            assert!(
                (sim - exact).abs() / exact < 1e-7,
                "θ0={th} L={l}: {sim} vs {exact}"
            );
        }
    }

    #[test]
    fn pipe_target_bounds_accept_simulator_and_reject_wrong_units() {
        let t = pipe_flow_target();
        let ctl = ExecutionControl::default();
        let x = [0.01, 0.05, 1e-5]; // Re = 500: laminar
        let dp = t.simulate(&x, &ctl).unwrap();
        assert!((t.bounds)(&t.input_map(&x), dp).is_ok());
        assert!(
            (t.bounds)(&t.input_map(&x), dp * 1e-3).is_err(),
            "kPa-for-Pa bug must violate Poiseuille in laminar flow"
        );
        assert_eq!(
            (t.regime_label)(&t.input_map(&[0.05, 1.0, 1e-5])),
            "turbulent"
        );
    }
}
