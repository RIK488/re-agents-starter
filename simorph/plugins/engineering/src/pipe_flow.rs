//! Darcy–Weisbach pressure drop in a circular pipe with Colebrook–White
//! friction factor (turbulent) or Poiseuille (laminar).

use simorph_core::*;
use serde_json::json;

pub struct PipeFlow {
    m: ComponentManifest,
}

impl PipeFlow {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("engineering.hydraulics.pipe_flow", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["energy", "process-engineering", "mining", "water"])
            .with_inputs(vec![
                Port::scalar("diameter", "m").with_range(1e-4, 10.0),
                Port::scalar("length", "m").with_range(0.0, 1e6),
                Port::scalar("velocity", "m/s").with_range(0.0, 100.0),
                Port::scalar("density", "kg/m^3").with_range(0.1, 2e4),
                Port::scalar("viscosity", "Pa*s").with_range(1e-7, 1e3),
                Port::scalar("roughness", "m").with_range(0.0, 1e-2),
            ])
            .with_outputs(vec![
                Port::scalar("reynolds", "1"),
                Port::scalar("friction_factor", "1"),
                Port::scalar("pressure_drop", "Pa"),
            ])
            .with_assumptions(&[
                "incompressible, isothermal, fully developed steady flow",
                "Colebrook–White correlation for Re > 4000; Poiseuille for Re < 2300; linear blend between",
            ])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "laminar Poiseuille".into(),
                    inputs: json!({"diameter": 0.01, "length": 1.0, "velocity": 0.1, "density": 1000.0, "viscosity": 1e-3, "roughness": 0.0}),
                    // Re = 1000 -> f = 64/Re = 0.064 ; dp = f L/D rho v²/2 = 0.064*100*1000*0.01/2 = 32 Pa
                    expected_outputs: json!({"reynolds": 1000.0, "friction_factor": 0.064, "pressure_drop": 32.0}),
                    rtol: 1e-9,
                    atol: 0.0,
                    source: "Hagen–Poiseuille closed form".into(),
                }],
                invariants: vec!["pressure_drop >= 0".into(), "friction_factor > 0".into()],
                convergence_order: None,
            });
        Self { m }
    }

    /// Colebrook–White solved by fixed-point iteration on 1/sqrt(f).
    pub fn colebrook(re: f64, rel_rough: f64) -> f64 {
        let mut x = -2.0 * (rel_rough / 3.7 + 5.74 / re.powf(0.9)).log10(); // Swamee–Jain start
        for _ in 0..50 {
            let next = -2.0 * (rel_rough / 3.7 + 2.51 * x / re).log10();
            if (next - x).abs() < 1e-12 {
                x = next;
                break;
            }
            x = next;
        }
        1.0 / (x * x)
    }
}

impl Default for PipeFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for PipeFlow {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let d = req.get_f64("diameter")?;
        let l = req.get_f64("length")?;
        let v = req.get_f64("velocity")?;
        let rho = req.get_f64("density")?;
        let mu = req.get_f64("viscosity")?;
        let eps = req.get_f64("roughness")?;
        let re = rho * v * d / mu;
        let mut warnings = vec![];
        let f = if re <= 0.0 {
            0.0
        } else if re < 2300.0 {
            64.0 / re
        } else if re > 4000.0 {
            Self::colebrook(re, eps / d)
        } else {
            warnings.push(format!("transitional regime Re={re:.0}: friction factor interpolated, high uncertainty"));
            let fl = 64.0 / 2300.0;
            let ft = Self::colebrook(4000.0, eps / d);
            fl + (ft - fl) * (re - 2300.0) / 1700.0
        };
        let dp = f * (l / d) * rho * v * v / 2.0;
        let mut res = TaskResult::new(req, EvidenceLevel::Computed)
            .output("reynolds", re)
            .output("friction_factor", f)
            .output("pressure_drop", dp);
        res.warnings = warnings;
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laminar_matches_poiseuille() {
        let c = PipeFlow::new();
        let req = TaskRequest::new(c.m.id)
            .input("diameter", 0.01)
            .input("length", 1.0)
            .input("velocity", 0.1)
            .input("density", 1000.0)
            .input("viscosity", 1e-3)
            .input("roughness", 0.0);
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        assert!((r.get_f64("pressure_drop").unwrap() - 32.0).abs() < 1e-9);
    }

    #[test]
    fn colebrook_satisfies_its_own_equation() {
        let re = 1e5;
        let rr = 1e-4;
        let f = PipeFlow::colebrook(re, rr);
        let lhs = 1.0 / f.sqrt();
        let rhs = -2.0 * (rr / 3.7 + 2.51 / (re * f.sqrt())).log10();
        assert!((lhs - rhs).abs() < 1e-9);
        // Moody chart sanity: smooth pipe at Re=1e5 gives f ≈ 0.018
        let fs = PipeFlow::colebrook(1e5, 0.0);
        assert!((fs - 0.018).abs() < 1e-3, "{fs}");
    }
}
