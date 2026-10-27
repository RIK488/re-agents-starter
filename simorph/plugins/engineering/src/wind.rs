//! Wind turbine power: P = ½ ρ A v³ Cp, with Cp ≤ Betz limit 16/27.

use serde_json::json;
use simorph_core::*;

pub struct WindTurbine {
    m: ComponentManifest,
}

impl WindTurbine {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("energy.wind.turbine_power", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["energy", "renewables"])
            .with_inputs(vec![
                Port::scalar("wind_speed", "m/s").with_range(0.0, 80.0),
                Port::scalar("rotor_diameter", "m").with_range(0.1, 300.0),
                Port::scalar("air_density", "kg/m^3").with_range(0.5, 1.5),
                Port::scalar("power_coefficient", "1").with_range(0.0, 16.0 / 27.0),
                Port::scalar("rated_power", "W").with_range(0.0, 5e7),
                Port::scalar("cut_in", "m/s").with_range(0.0, 20.0),
                Port::scalar("cut_out", "m/s").with_range(0.0, 80.0),
            ])
            .with_outputs(vec![Port::scalar("power", "W"), Port::scalar("available_power", "W")])
            .with_assumptions(&["steady uniform inflow", "Cp constant (no tip-speed-ratio dependence)", "power clipped at rated power; zero outside [cut_in, cut_out]"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "10 m/s, D=100 m, Cp=0.4".into(),
                    inputs: json!({"wind_speed": 10.0, "rotor_diameter": 100.0, "air_density": 1.225, "power_coefficient": 0.4, "rated_power": 1e7, "cut_in": 3.0, "cut_out": 25.0}),
                    expected_outputs: json!({"power": 0.5 * 1.225 * std::f64::consts::PI * 2500.0 * 1000.0 * 0.4}),
                    rtol: 1e-12,
                    atol: 0.0,
                    source: "actuator-disc power formula".into(),
                }],
                invariants: vec!["power <= available_power * 0.5926 + 1e-9".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for WindTurbine {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for WindTurbine {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let v = req.get_f64("wind_speed")?;
        let d = req.get_f64("rotor_diameter")?;
        let rho = req.get_f64("air_density")?;
        let cp = req.get_f64("power_coefficient")?;
        let rated = req.get_f64("rated_power")?;
        let cin = req.get_f64("cut_in")?;
        let cout = req.get_f64("cut_out")?;
        let area = std::f64::consts::PI * (d / 2.0).powi(2);
        let available = 0.5 * rho * area * v.powi(3);
        let p = if v < cin || v > cout {
            0.0
        } else {
            (available * cp).min(rated)
        };
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("power", p)
            .output("available_power", available))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_law_and_cutoffs() {
        let c = WindTurbine::new();
        let base = |v: f64| {
            TaskRequest::new(c.m.id)
                .input("wind_speed", v)
                .input("rotor_diameter", 100.0)
                .input("air_density", 1.225)
                .input("power_coefficient", 0.4)
                .input("rated_power", 1e7)
                .input("cut_in", 3.0)
                .input("cut_out", 25.0)
        };
        let p10 = run_component(&c, &base(10.0), &ExecutionControl::default())
            .unwrap()
            .get_f64("power")
            .unwrap();
        let p5 = run_component(&c, &base(5.0), &ExecutionControl::default())
            .unwrap()
            .get_f64("power")
            .unwrap();
        assert!((p10 / p5 - 8.0).abs() < 1e-9);
        assert_eq!(
            run_component(&c, &base(30.0), &ExecutionControl::default())
                .unwrap()
                .get_f64("power")
                .unwrap(),
            0.0
        );
        // Betz: Cp above 16/27 is outside the port range and refused
        let bad = base(10.0).input("power_coefficient", 0.7);
        assert!(run_component(&c, &bad, &ExecutionControl::default()).is_err());
    }
}
