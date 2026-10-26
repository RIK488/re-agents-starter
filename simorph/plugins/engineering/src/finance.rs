//! Net present value and internal rate of return of a cash-flow series.

use simorph_core::*;
use serde_json::json;

pub struct NetPresentValue {
    m: ComponentManifest,
}

impl NetPresentValue {
    pub fn new() -> Self {
        let m = ComponentManifest::in_process("economics.finance.npv", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["economics", "finance", "project-evaluation"])
            .with_inputs(vec![
                Port { name: "cash_flows".into(), unit: "1".into(), description: Some("cash flow per period, index 0 = today (currency units)".into()), shape: vec![0], range: None },
                Port::scalar("discount_rate", "1").with_range(-0.99, 10.0),
            ])
            .with_outputs(vec![Port::scalar("npv", "1"), Port::scalar("irr", "1"), Port::scalar("payback_period", "1")])
            .with_assumptions(&[
                "cash flows occur at the end of equal periods",
                "constant discount rate",
                "IRR reported only if a sign change exists and bisection converges; otherwise NaN with a warning",
            ])
            .with_validation(ValidationProtocol {
                reference_cases: vec![ReferenceCase {
                    name: "two-period investment".into(),
                    inputs: json!({"cash_flows": [-100.0, 60.0, 60.0], "discount_rate": 0.1}),
                    expected_outputs: json!({"npv": -100.0 + 60.0/1.1 + 60.0/1.21}),
                    rtol: 1e-12,
                    atol: 0.0,
                    source: "definition of NPV".into(),
                }],
                invariants: vec![],
                convergence_order: None,
            });
        Self { m }
    }

    pub fn npv(flows: &[f64], r: f64) -> f64 {
        flows.iter().enumerate().map(|(t, cf)| cf / (1.0 + r).powi(t as i32)).sum()
    }

    pub fn irr(flows: &[f64]) -> Option<f64> {
        let (mut lo, mut hi) = (-0.99, 10.0);
        let flo = Self::npv(flows, lo);
        let fhi = Self::npv(flows, hi);
        if flo.signum() == fhi.signum() {
            return None;
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            let fm = Self::npv(flows, mid);
            if fm.abs() < 1e-12 {
                return Some(mid);
            }
            if fm.signum() == flo.signum() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(0.5 * (lo + hi))
    }
}

impl Default for NetPresentValue {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for NetPresentValue {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let flows = req.get_vec_f64("cash_flows")?;
        let r = req.get_f64("discount_rate")?;
        if flows.is_empty() {
            return Err(SimorphError::InvalidInput("cash_flows empty".into()));
        }
        let npv = Self::npv(&flows, r);
        let mut res = TaskResult::new(req, EvidenceLevel::Computed).output("npv", npv);
        match Self::irr(&flows) {
            Some(i) => res = res.output("irr", i),
            None => {
                res = res.output("irr", f64::NAN);
                res.warnings.push("IRR undefined: no sign change of NPV over [-99%, 1000%]".into());
            }
        }
        let mut cum = 0.0;
        let mut payback = f64::NAN;
        for (t, cf) in flows.iter().enumerate() {
            let prev = cum;
            cum += cf;
            if prev < 0.0 && cum >= 0.0 && t > 0 {
                payback = (t - 1) as f64 + (-prev) / cf;
                break;
            }
        }
        Ok(res.output("payback_period", payback))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npv_irr_payback() {
        let c = NetPresentValue::new();
        let req = TaskRequest::new(c.m.id).input("cash_flows", json!([-100.0, 60.0, 60.0])).input("discount_rate", 0.1);
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        assert!((r.get_f64("npv").unwrap() - (-100.0 + 60.0 / 1.1 + 60.0 / 1.21)).abs() < 1e-12);
        // IRR of [-100, 60, 60]: 60/(1+i) + 60/(1+i)^2 = 100 -> i = 0.1306623...
        assert!((r.get_f64("irr").unwrap() - 0.130_662_3).abs() < 1e-6);
        assert!((r.get_f64("payback_period").unwrap() - (1.0 + 40.0 / 60.0)).abs() < 1e-12);
    }
}
