//! M/M/1 queue steady-state metrics.

use simorph_core::*;
use serde_json::json;

pub struct Mm1Queue {
    m: ComponentManifest,
}

impl Mm1Queue {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("operations.queue.mm1", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver, Capability::ProbabilisticInference])
            .with_domains(&["logistics", "telecommunications", "computing", "operations-research"])
            .with_inputs(vec![Port::scalar("arrival_rate", "1/s").with_range(0.0, 1e9), Port::scalar("service_rate", "1/s").with_range(1e-12, 1e9)])
            .with_outputs(vec![
                Port::scalar("utilization", "1"),
                Port::scalar("mean_customers", "1"),
                Port::scalar("mean_wait", "s"),
                Port::scalar("mean_sojourn", "s"),
            ])
            .with_assumptions(&["Poisson arrivals, exponential service, single server, infinite buffer, FIFO", "steady state exists only if arrival_rate < service_rate"])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "rho = 0.5".into(),
                    inputs: json!({"arrival_rate": 1.0, "service_rate": 2.0}),
                    expected_outputs: json!({"utilization": 0.5, "mean_customers": 1.0, "mean_sojourn": 1.0, "mean_wait": 0.5}),
                    rtol: 1e-12,
                    atol: 0.0,
                    source: "Kendall M/M/1 closed form".into(),
                }],
                invariants: vec!["utilization < 1".into()],
                convergence_order: None,
            });
        Self { m }
    }
}

impl Default for Mm1Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Mm1Queue {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let lam = req.get_f64("arrival_rate")?;
        let mu = req.get_f64("service_rate")?;
        let rho = lam / mu;
        if rho >= 1.0 {
            return Err(SimorphError::ValidityDomain(format!("utilization {rho:.3} >= 1: queue is unstable, no steady state")));
        }
        let l = rho / (1.0 - rho);
        let w = l / lam.max(f64::MIN_POSITIVE);
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("utilization", rho)
            .output("mean_customers", l)
            .output("mean_sojourn", if lam > 0.0 { w } else { 1.0 / mu })
            .output("mean_wait", if lam > 0.0 { w - 1.0 / mu } else { 0.0 }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_form_and_instability() {
        let c = Mm1Queue::new();
        let r = run_component(&c, &TaskRequest::new(c.m.id).input("arrival_rate", 1.0).input("service_rate", 2.0), &ExecutionControl::default()).unwrap();
        assert_eq!(r.get_f64("mean_customers").unwrap(), 1.0);
        assert_eq!(r.get_f64("mean_wait").unwrap(), 0.5);
        assert!(run_component(&c, &TaskRequest::new(c.m.id).input("arrival_rate", 3.0).input("service_rate", 2.0), &ExecutionControl::default()).is_err());
    }
}
