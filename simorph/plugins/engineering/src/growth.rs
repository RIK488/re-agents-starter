//! Logistic growth closed form: N(t) = K / (1 + (K/N0 − 1) e^{−rt}).

use serde_json::json;
use simorph_core::*;

pub struct LogisticGrowth {
    m: ComponentManifest,
}

impl LogisticGrowth {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("biology.population.logistic", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["biology", "agriculture", "epidemiology", "ecology", "economics"])
            .with_inputs(vec![
                Port::scalar("initial_population", "1").with_range(0.0, f64::MAX),
                Port::scalar("carrying_capacity", "1").with_range(1e-12, f64::MAX),
                Port::scalar("growth_rate", "1/s"),
                Port::scalar("time", "s").with_range(0.0, f64::MAX),
            ])
            .with_outputs(vec![Port::scalar("population", "1"), Port::scalar("time_to_half_capacity", "s")])
            .with_assumptions(&["homogeneous population, constant parameters, no delay, no stochasticity"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "half capacity time".into(),
                    inputs: json!({"initial_population": 10.0, "carrying_capacity": 1000.0, "growth_rate": 0.1, "time": 0.0}),
                    expected_outputs: json!({"population": 10.0, "time_to_half_capacity": (99.0f64).ln() / 0.1}),
                    rtol: 1e-12,
                    atol: 0.0,
                    source: "closed-form logistic solution".into(),
                }],
                invariants: vec!["population >= 0".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for LogisticGrowth {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for LogisticGrowth {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let n0 = req.get_f64("initial_population")?;
        let k = req.get_f64("carrying_capacity")?;
        let r = req.get_f64("growth_rate")?;
        let t = req.get_f64("time")?;
        let n = if n0 == 0.0 {
            0.0
        } else {
            k / (1.0 + (k / n0 - 1.0) * (-r * t).exp())
        };
        let t_half = if n0 > 0.0 && n0 < k / 2.0 && r > 0.0 {
            (k / n0 - 1.0).ln() / r
        } else {
            f64::NAN
        };
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("population", n)
            .output("time_to_half_capacity", t_half))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_capacity_and_half_time() {
        let c = LogisticGrowth::new();
        let req = |t: f64| {
            TaskRequest::new(c.m.id)
                .input("initial_population", 10.0)
                .input("carrying_capacity", 1000.0)
                .input("growth_rate", 0.1)
                .input("time", t)
        };
        let r = run_component(&c, &req(0.0), &ExecutionControl::default()).unwrap();
        let th = r.get_f64("time_to_half_capacity").unwrap();
        let at_half = run_component(&c, &req(th), &ExecutionControl::default())
            .unwrap()
            .get_f64("population")
            .unwrap();
        assert!((at_half - 500.0).abs() < 1e-9);
        let late = run_component(&c, &req(1e4), &ExecutionControl::default())
            .unwrap()
            .get_f64("population")
            .unwrap();
        assert!((late - 1000.0).abs() < 1e-6);
    }
}
