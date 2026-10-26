//! Equivalent-circuit battery: open-circuit voltage `OCV(SOC)` in series
//! with an internal resistance `R`.
//!
//! State: `soc` (1), `e_stored` (J, ∫OCV·I dt), `e_loss` (J, ∫R·I² dt).
//! Net electrical power into the battery `P = power_in − power_out` gives the
//! current from `P = (OCV + R I) I`:
//! `I = (−OCV + √(OCV² + 4 R P)) / (2R)` (→ `P/OCV` when `R = 0`).
//! `dSOC/dt = I / (3600 · capacity_ah)`.
//!
//! OCV is linear in SOC between `v_ocv_min` and `v_ocv_max` (documented
//! simplification; a tabulated OCV is a straightforward extension).

use indexmap::IndexMap;
use simorph_core::{ReferenceCase, Result, SimorphError};
use simorph_cosimulation::OdeSlave;
use simorph_simulation::model::param;
use simorph_simulation::{DynamicalModel, OdeComponent, Params};
use std::sync::Arc;

pub const NAME: &str = "physics.battery.equivalent_circuit";

pub struct Battery;

impl Battery {
    pub fn ocv(soc: f64, p: &Params) -> Result<f64> {
        let (lo, hi) = (param(p, "v_ocv_min")?, param(p, "v_ocv_max")?);
        Ok(lo + (hi - lo) * soc.clamp(0.0, 1.0))
    }
    /// Current (A, positive = charging) for a net power `p_net` (W), with
    /// the battery-management cut-offs: no charging at SOC ≥ 1 (excess power
    /// is shunted) and no discharging at SOC ≤ 0 (load shed).
    pub fn current_bms(soc: f64, ocv: f64, r: f64, p_net: f64) -> Result<f64> {
        if (soc >= 1.0 && p_net > 0.0) || (soc <= 0.0 && p_net < 0.0) {
            return Ok(0.0);
        }
        Self::current(ocv, r, p_net)
    }
    /// Current (A, positive = charging) for a net power `p_net` (W).
    pub fn current(ocv: f64, r: f64, p_net: f64) -> Result<f64> {
        if r <= 0.0 {
            return Ok(p_net / ocv);
        }
        let disc = ocv * ocv + 4.0 * r * p_net;
        if disc < 0.0 {
            return Err(SimorphError::ValidityDomain(format!(
                "discharge power {:.3} W exceeds the maximum deliverable OCV²/4R = {:.3} W",
                -p_net,
                ocv * ocv / (4.0 * r)
            )));
        }
        Ok((-ocv + disc.sqrt()) / (2.0 * r))
    }
}

impl DynamicalModel for Battery {
    fn name(&self) -> &str {
        NAME
    }
    fn state_dim(&self) -> usize {
        3
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![
            ("soc".into(), "1".into()),
            ("e_stored".into(), "J".into()),
            ("e_loss".into(), "J".into()),
        ]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("capacity_ah".into(), "Ah".into()),
            ("r_internal".into(), "ohm".into()),
            ("v_ocv_min".into(), "V".into()),
            ("v_ocv_max".into(), "V".into()),
            ("soc0".into(), "1".into()),
            ("power_in".into(), "W".into()),
            ("power_out".into(), "W".into()),
        ]
    }
    fn output_names(&self) -> Vec<(String, String)> {
        vec![
            ("voltage".into(), "V".into()),
            ("current".into(), "A".into()),
            ("power_net".into(), "W".into()),
        ]
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        vec![p.get("soc0").copied().unwrap_or(0.5), 0.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        let ocv = Self::ocv(x[0], p)?;
        let r = param(p, "r_internal")?;
        let p_net = param(p, "power_in")? - param(p, "power_out")?;
        let i = Self::current_bms(x[0], ocv, r, p_net)?;
        dx[0] = i / (3600.0 * param(p, "capacity_ah")?);
        dx[1] = ocv * i;
        dx[2] = r * i * i;
        Ok(())
    }
    fn outputs(&self, _t: f64, x: &[f64], p: &Params) -> IndexMap<String, f64> {
        let ocv = Self::ocv(x[0], p).unwrap_or(f64::NAN);
        let r = p.get("r_internal").copied().unwrap_or(0.0);
        let p_net =
            p.get("power_in").copied().unwrap_or(0.0) - p.get("power_out").copied().unwrap_or(0.0);
        let i = Self::current_bms(x[0], ocv, r, p_net).unwrap_or(f64::NAN);
        IndexMap::from([
            ("voltage".to_string(), ocv + r * i),
            ("current".to_string(), i),
            ("power_net".to_string(), p_net),
        ])
    }
    fn assumptions(&self) -> Vec<String> {
        vec![
            "Zeroth-order equivalent circuit: OCV source in series with a constant internal resistance (no RC polarisation branches, no temperature or ageing effects).".into(),
            "OCV varies linearly with state of charge.".into(),
            "Coulombic efficiency is 100 %.".into(),
            "Ideal battery management: charging stops at SOC = 1 (excess power shunted) and discharging stops at SOC = 0 (load shed).".into(),
        ]
    }
    fn validity_ranges(&self) -> Vec<(String, (f64, f64))> {
        vec![
            ("soc".into(), (0.0, 1.0)),
            ("r_internal".into(), (0.0, 10.0)),
            ("capacity_ah".into(), (1e-3, 1e4)),
        ]
    }
    fn domains(&self) -> Vec<String> {
        vec![
            "electrical".into(),
            "energy-storage".into(),
            "aerospace".into(),
        ]
    }
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        vec![ReferenceCase {
            name: "Coulomb counting at constant power with zero resistance".into(),
            inputs: serde_json::json!({"capacity_ah": 10.0, "r_internal": 0.0, "v_ocv_min": 3.6, "v_ocv_max": 3.6, "soc0": 0.5,
                "power_in": 36.0, "power_out": 0.0, "t_end": 1800.0}),
            expected_outputs: serde_json::json!({"final_soc": 1.0, "final_e_stored": 36.0 * 1800.0}),
            rtol: 1e-8,
            atol: 1e-9,
            source: "ΔSOC = I·t / (3600·C), I = P/OCV".into(),
        }]
    }
}

pub fn component() -> Box<dyn simorph_core::Component> {
    Box::new(OdeComponent::with_defaults(
        Arc::new(Battery),
        crate::VERSION,
        crate::PROVIDER,
        crate::default_solver(),
    ))
}

/// Slave with inputs `power_in`, `power_out` (W) and outputs `soc` (1),
/// `voltage` (V), `current` (A), `e_stored`, `e_loss` (J), `power_net` (W).
/// Outputs `voltage`/`current` depend algebraically on the inputs, hence
/// direct feedthrough is declared.
pub fn slave() -> Result<OdeSlave> {
    OdeSlave::new(
        Arc::new(Battery),
        &["power_in", "power_out"],
        crate::default_solver(),
        true,
    )
}
