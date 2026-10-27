//! Nanosatellite power/thermal/orbit/attitude co-simulation exposed as a
//! single `Component`, so that the scientific director can treat the whole
//! multidisciplinary system as one simulator (design the array area and the
//! battery, check the energy budget over N orbits).
//!
//! Outputs are aggregates of the co-simulated signals (minimum state of
//! charge, eclipse fraction, energy balance error, temperature extremes),
//! all `Simulated`.  The energy balance closure is itself exposed so the
//! validation engine can check it as an invariant.

use crate::demo::{nanosat_coupling, NanosatParams};
use crate::orbit::{TwoBody, MU_EARTH, R_EARTH};
use serde_json::json;
use simorph_core::*;
use simorph_cosimulation::{run, MasterAlgorithm, MasterOptions};

pub const NAME: &str = "physics.nanosat.power_thermal_cosim";

pub struct NanosatCosim {
    m: ComponentManifest,
}

impl NanosatCosim {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process(NAME, crate::VERSION, crate::PROVIDER)
            .with_capabilities(vec![Capability::OdeIntegration, Capability::CosimulationSlave])
            .with_domains(&["aerospace", "nanosatellite", "power", "thermal", "attitude"])
            .with_inputs(vec![
                Port::scalar("altitude", "m").with_range(200e3, 2000e3),
                Port::scalar("array_area", "m^2").with_range(0.005, 2.0),
                Port::scalar("array_efficiency", "1").with_range(0.05, 0.45),
                Port::scalar("load_power", "W").with_range(0.1, 500.0),
                Port::scalar("battery_capacity_ah", "Ah").with_range(0.2, 200.0),
                Port::scalar("soc0", "1").with_range(0.05, 1.0),
                Port::scalar("orbits", "1").with_range(0.1, 50.0),
            ])
            .with_outputs(vec![
                Port::scalar("min_soc", "1"),
                Port::scalar("final_soc", "1"),
                Port::scalar("eclipse_fraction", "1"),
                Port::scalar("mean_solar_power", "W"),
                Port::scalar("energy_balance_error", "1"),
                Port::scalar("max_array_temperature", "K"),
                Port::scalar("min_array_temperature", "K"),
                Port::scalar("curtailed_energy", "J"),
                Port::scalar("array_mass", "kg"),
                Port::scalar("orbital_period", "s"),
            ])
            .with_assumptions(&[
                "circular equatorial orbit, cylindrical Earth shadow, no penumbra",
                "array always sun-pointing when lit (cos incidence = 1)",
                "constant platform load; linear OCV battery; two-node thermal network",
                "array areal mass 3.5 kg/m² (typical body-mounted GaAs panel with substrate)",
                "macro step 10 s, Gauss–Seidel master; sub-step RK45 inside each slave",
            ])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "orbital period and eclipse fraction at 500 km".into(),
                    inputs: json!({"altitude": 500e3, "array_area": 0.06, "array_efficiency": 0.28, "load_power": 14.0, "battery_capacity_ah": 5.0, "soc0": 0.5, "orbits": 1.0}),
                    expected_outputs: json!({
                        "orbital_period": TwoBody::circular_period(R_EARTH + 500e3, MU_EARTH),
                        "eclipse_fraction": (R_EARTH / (R_EARTH + 500e3)).asin() / std::f64::consts::PI,
                    }),
                    rtol: 0.02,
                    atol: 0.0,
                    source: "Kepler's third law; cylindrical-shadow eclipse fraction asin(R/a)/π".into(),
                }],
                invariants: vec![
                    "min_soc >= 0".into(),
                    "final_soc <= 1.000001".into(),
                    "abs(energy_balance_error) < 0.05".into(),
                    "max_array_temperature < 400".into(),
                ],
                convergence_order: None,
            });
        let mut m = m;
        m.uncertainty_model = UncertaintyModel::Described { description: "co-simulation macro-step error estimated by step doubling (<1 % on SOC at 10 s); model-form uncertainty undeclared".into() };
        m.deterministic_mode = DeterministicMode::Always;
        m.resource_requirements.wall_time_s = 0.5;
        Self { m }
    }
}

impl Default for NanosatCosim {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for NanosatCosim {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let p = NanosatParams {
            altitude: req.get_f64("altitude")?,
            inclination: 0.0,
            array_area: req.get_f64("array_area")?,
            array_efficiency: req.get_f64("array_efficiency")?,
            load_power: req.get_f64("load_power")?,
            battery_capacity_ah: req.get_f64("battery_capacity_ah")?,
            soc0: req.get_f64("soc0")?,
        };
        let orbits = req.get_f64_or("orbits", 1.0);
        let period = TwoBody::circular_period(R_EARTH + p.altitude, MU_EARTH);
        // fidelity < 1 → coarser macro step (cheaper, less accurate)
        let macro_step = 10.0 * (1.0 + 5.0 * (1.0 - req.fidelity.clamp(0.0, 1.0)));
        let mut c = nanosat_coupling(&p)?;
        let tr = run(
            &mut c,
            &MasterOptions::new(MasterAlgorithm::GaussSeidel, period * orbits, macro_step),
            ctl,
        )?;
        let sig = |n: &str| {
            tr.signal(n)
                .ok_or_else(|| SimorphError::NotFound(format!("signal {n}")))
        };
        let soc = sig("battery.soc")?;
        let ecl = sig("orbit.eclipse")?;
        let power = sig("solar.power")?;
        let t0 = sig("thermal.temperature_0")?;
        let dt = if tr.times.len() > 1 {
            tr.times[1] - tr.times[0]
        } else {
            macro_step
        };
        let e_stored = *sig("battery.e_stored")?.last().unwrap_or(&0.0);
        let e_loss = *sig("battery.e_loss")?.last().unwrap_or(&0.0);
        // Accepted electrical power is V·I (the BMS shunts excess power at SOC = 1
        // and sheds load at SOC = 0); the requested net power may differ.
        let v = sig("battery.voltage")?;
        let i = sig("battery.current")?;
        let requested = sig("battery.power_net")?;
        let accepted: f64 = v.iter().zip(i).skip(1).map(|(v, i)| v * i * dt).sum();
        let curtailed: f64 = requested
            .iter()
            .zip(v.iter().zip(i))
            .skip(1)
            .map(|(p, (v, i))| (p - v * i).max(0.0) * dt)
            .sum();
        let balance_err = (e_stored + e_loss - accepted) / accepted.abs().max(1.0);
        let min_soc = soc.iter().cloned().fold(f64::INFINITY, f64::min);
        let mut res = TaskResult::new(req, EvidenceLevel::Simulated)
            .output("min_soc", min_soc)
            .output("final_soc", *soc.last().unwrap_or(&f64::NAN))
            .output(
                "eclipse_fraction",
                ecl.iter().sum::<f64>() / ecl.len().max(1) as f64,
            )
            .output(
                "mean_solar_power",
                power.iter().sum::<f64>() / power.len().max(1) as f64,
            )
            .output("energy_balance_error", balance_err)
            .output(
                "max_array_temperature",
                t0.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            )
            .output(
                "min_array_temperature",
                t0.iter().cloned().fold(f64::INFINITY, f64::min),
            )
            .output("curtailed_energy", curtailed)
            .output("array_mass", 3.5 * p.array_area)
            .output("orbital_period", period)
            .metric("macro_steps", tr.macro_steps as f64)
            .metric("macro_step_s", macro_step)
            .metric("fixed_point_iterations", tr.fixed_point_iterations as f64);
        if min_soc <= 0.0 {
            res.warnings
                .push("battery fully depleted during the simulation: platform brown-out".into());
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanosat_component_matches_reference_and_scales_with_area() {
        let c = NanosatCosim::new();
        let ctl = ExecutionControl::default();
        let base = |area: f64| {
            TaskRequest::new(c.m.id)
                .input("altitude", 500e3)
                .input("array_area", area)
                .input("array_efficiency", 0.28)
                .input("load_power", 14.0)
                .input("battery_capacity_ah", 5.0)
                .input("soc0", 0.5)
                .input("orbits", 1.0)
        };
        let small = run_component(&c, &base(0.03), &ctl).unwrap();
        let big = run_component(&c, &base(0.12), &ctl).unwrap();
        assert!(big.get_f64("final_soc").unwrap() > small.get_f64("final_soc").unwrap());
        assert!(big.get_f64("energy_balance_error").unwrap().abs() < 0.05);
        let expected = (R_EARTH / (R_EARTH + 500e3)).asin() / std::f64::consts::PI;
        assert!((big.get_f64("eclipse_fraction").unwrap() - expected).abs() < 0.02);
        // low fidelity is cheaper (fewer macro steps) and close
        let coarse = run_component(&c, &base(0.12).fidelity(0.2), &ctl).unwrap();
        assert!(coarse.metrics["macro_steps"] < big.metrics["macro_steps"]);
        assert!(
            (coarse.get_f64("final_soc").unwrap() - big.get_f64("final_soc").unwrap()).abs() < 0.05
        );
    }
}
