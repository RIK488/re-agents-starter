//! Euler–Bernoulli cantilever beam with a point load at the free end.

use simorph_core::*;
use serde_json::json;

pub struct Cantilever {
    m: ComponentManifest,
}

impl Cantilever {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("engineering.beam.cantilever", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["civil-engineering", "mechanical-engineering", "construction"])
            .with_inputs(vec![
                Port::scalar("length", "m").with_range(1e-3, 1e3),
                Port::scalar("load", "N"),
                Port::scalar("youngs_modulus", "Pa").with_range(1e6, 1e13),
                Port::scalar("second_moment", "m^4").with_range(1e-15, 1e3),
            ])
            .with_outputs(vec![
                Port::scalar("tip_deflection", "m"),
                Port::scalar("max_bending_moment", "N*m"),
                Port::scalar("tip_slope", "rad"),
            ])
            .with_assumptions(&[
                "linear elastic material",
                "small deflections (tip deflection << length)",
                "slender beam: shear deformation neglected (Euler–Bernoulli)",
                "point load applied at the free end, perpendicular to the axis",
            ])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "steel 2 m cantilever, 1 kN".into(),
                    inputs: json!({"length": 2.0, "load": 1000.0, "youngs_modulus": 2.1e11, "second_moment": 8.33e-6}),
                    // δ = P L³ / (3 E I)
                    expected_outputs: json!({"tip_deflection": 1000.0 * 8.0 / (3.0 * 2.1e11 * 8.33e-6)}),
                    rtol: 1e-9,
                    atol: 0.0,
                    source: "closed-form Euler–Bernoulli solution".into(),
                }],
                invariants: vec!["tip_deflection >= 0".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for Cantilever {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Cantilever {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let l = req.get_f64("length")?;
        let p = req.get_f64("load")?;
        let e = req.get_f64("youngs_modulus")?;
        let i = req.get_f64("second_moment")?;
        let delta = p * l.powi(3) / (3.0 * e * i);
        let slope = p * l.powi(2) / (2.0 * e * i);
        let mut res = TaskResult::new(req, EvidenceLevel::Computed)
            .output("tip_deflection", delta)
            .output("max_bending_moment", p * l)
            .output("tip_slope", slope);
        if delta.abs() > 0.1 * l {
            res.warnings.push(format!(
                "tip deflection {delta:.3e} m exceeds 10% of length: small-deflection assumption violated"
            ));
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_closed_form_and_flags_large_deflection() {
        let c = Cantilever::new();
        let req = TaskRequest::new(c.m.id)
            .input("length", 2.0)
            .input("load", 1000.0)
            .input("youngs_modulus", 2.1e11)
            .input("second_moment", 8.33e-6);
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        let expected = 1000.0 * 8.0 / (3.0 * 2.1e11 * 8.33e-6);
        assert!((r.get_f64("tip_deflection").unwrap() - expected).abs() < 1e-12);
        assert!(r.warnings.is_empty());
        let soft = TaskRequest::new(c.m.id)
            .input("length", 2.0)
            .input("load", 1e6)
            .input("youngs_modulus", 2.1e9)
            .input("second_moment", 8.33e-6);
        let r = run_component(&c, &soft, &ExecutionControl::default()).unwrap();
        assert!(!r.warnings.is_empty());
    }
}
