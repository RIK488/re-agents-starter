//! Lumped-parameter thermal network.
//!
//! For node `i` with capacitance `C_i`:
//! `C_i dT_i/dt = heat_in_i + Σ_j G_i_j (T_j − T_i) − G_env_i (T_i − T_env) − epsA_i σ (T_i⁴ − T_space⁴)`.
//!
//! Parameter names: `C_<i>` (J/K), `G_<i>_<j>` (W/K, one per declared link,
//! `i<j`), `heat_in_<i>` (W), `G_env_<i>` (W/K), `T_env` (K), `T0_<i>` (K)
//! and, when radiation is enabled, `epsA_<i>` (m²) and `T_space` (K).
//! State/output names: `temperature_<i>` (K).

use simorph_core::{ReferenceCase, Result};
use simorph_cosimulation::OdeSlave;
use simorph_simulation::model::param;
use simorph_simulation::{DynamicalModel, OdeComponent, Params};
use std::sync::Arc;

pub const NAME: &str = "physics.thermal.network";
/// Stefan–Boltzmann constant (W m⁻² K⁻⁴).
pub const SIGMA: f64 = 5.670_374_419e-8;

#[derive(Debug, Clone)]
pub struct ThermalNetwork {
    pub n: usize,
    /// Conductive links `(i, j)` with `i < j`.
    pub links: Vec<(usize, usize)>,
    pub radiation: bool,
}

impl ThermalNetwork {
    /// Chain topology `0-1-2-…-(n-1)`.
    pub fn chain(n: usize, radiation: bool) -> Self {
        Self {
            n,
            links: (0..n.saturating_sub(1)).map(|i| (i, i + 1)).collect(),
            radiation,
        }
    }
    fn link_name(i: usize, j: usize) -> String {
        format!("G_{i}_{j}")
    }
    /// Steady-state temperatures without radiation, from the linear system
    /// `(diag(G_env) + L) T = heat_in + G_env T_env`.
    pub fn linear_steady_state(&self, p: &Params) -> Result<Vec<f64>> {
        let n = self.n;
        let mut a = vec![0.0; n * n];
        let mut b = vec![0.0; n];
        let t_env = param(p, "T_env")?;
        for i in 0..n {
            let g_env = param(p, &format!("G_env_{i}"))?;
            a[i * n + i] += g_env;
            b[i] = param(p, &format!("heat_in_{i}"))? + g_env * t_env;
        }
        for &(i, j) in &self.links {
            let g = param(p, &Self::link_name(i, j))?;
            a[i * n + i] += g;
            a[j * n + j] += g;
            a[i * n + j] -= g;
            a[j * n + i] -= g;
        }
        simorph_simulation::linalg::solve_dense(&mut a, &mut b)?;
        Ok(b)
    }
}

impl DynamicalModel for ThermalNetwork {
    fn name(&self) -> &str {
        NAME
    }
    fn state_dim(&self) -> usize {
        self.n
    }
    fn state_names(&self) -> Vec<(String, String)> {
        (0..self.n)
            .map(|i| (format!("temperature_{i}"), "K".to_string()))
            .collect()
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        let mut v = vec![("T_env".to_string(), "K".to_string())];
        for i in 0..self.n {
            v.push((format!("C_{i}"), "J/K".into()));
            v.push((format!("heat_in_{i}"), "W".into()));
            v.push((format!("G_env_{i}"), "W/K".into()));
            v.push((format!("T0_{i}"), "K".into()));
        }
        for &(i, j) in &self.links {
            v.push((Self::link_name(i, j), "W/K".into()));
        }
        if self.radiation {
            v.push(("T_space".into(), "K".into()));
            for i in 0..self.n {
                v.push((format!("epsA_{i}"), "m^2".into()));
            }
        }
        v
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        (0..self.n)
            .map(|i| p.get(&format!("T0_{i}")).copied().unwrap_or(293.15))
            .collect()
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        let t_env = param(p, "T_env")?;
        for i in 0..self.n {
            let g_env = param(p, &format!("G_env_{i}"))?;
            let mut q = param(p, &format!("heat_in_{i}"))? - g_env * (x[i] - t_env);
            if self.radiation {
                let t_space = param(p, "T_space")?;
                q -= param(p, &format!("epsA_{i}"))? * SIGMA * (x[i].powi(4) - t_space.powi(4));
            }
            dx[i] = q;
        }
        for &(i, j) in &self.links {
            let g = param(p, &Self::link_name(i, j))?;
            let flow = g * (x[j] - x[i]);
            dx[i] += flow;
            dx[j] -= flow;
        }
        for i in 0..self.n {
            dx[i] /= param(p, &format!("C_{i}"))?;
        }
        Ok(())
    }
    fn assumptions(&self) -> Vec<String> {
        vec![
            "Each node is isothermal (lumped capacitance, Biot number ≪ 1).".into(),
            "Conductances and capacitances are temperature independent.".into(),
            "Radiation, when enabled, is grey-body exchange with a black sink at T_space (no view-factor coupling between nodes).".into(),
        ]
    }
    fn validity_ranges(&self) -> Vec<(String, (f64, f64))> {
        (0..self.n)
            .map(|i| (format!("temperature_{i}"), (0.0, 5000.0)))
            .collect()
    }
    fn domains(&self) -> Vec<String> {
        vec!["thermal".into(), "aerospace".into(), "engineering".into()]
    }
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        if self.n != 2 || self.radiation {
            return vec![];
        }
        // Two equal nodes coupled by G, isolated: T1-T2 decays as exp(-2Gt/C).
        let (c, g, t): (f64, f64, f64) = (1000.0, 5.0, 200.0);
        let d = 20.0 * (-2.0 * g * t / c).exp();
        vec![ReferenceCase {
            name: "two isolated coupled nodes relax exponentially".into(),
            inputs: serde_json::json!({"T_env": 0.0, "C_0": c, "C_1": c, "heat_in_0": 0.0, "heat_in_1": 0.0, "G_env_0": 0.0, "G_env_1": 0.0,
                "T0_0": 310.0, "T0_1": 290.0, "G_0_1": g, "t_end": t}),
            expected_outputs: serde_json::json!({"final_temperature_0": 300.0 + d / 2.0, "final_temperature_1": 300.0 - d / 2.0}),
            rtol: 1e-6,
            atol: 1e-4,
            source: "analytic solution of the 2-node linear system".into(),
        }]
    }
}

pub fn component(n: usize) -> Box<dyn simorph_core::Component> {
    Box::new(OdeComponent::with_defaults(
        Arc::new(ThermalNetwork::chain(n, false)),
        crate::VERSION,
        crate::PROVIDER,
        crate::default_solver(),
    ))
}

/// Chain network slave with `heat_in_<i>` (W) inputs and `temperature_<i>` (K) outputs.
pub fn slave(n: usize) -> Result<OdeSlave> {
    slave_for(ThermalNetwork::chain(n, false))
}

pub fn slave_for(net: ThermalNetwork) -> Result<OdeSlave> {
    let names: Vec<String> = (0..net.n).map(|i| format!("heat_in_{i}")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    OdeSlave::new(Arc::new(net), &refs, crate::default_solver(), false)
}
