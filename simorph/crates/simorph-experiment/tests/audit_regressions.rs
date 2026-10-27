//! Regression tests for the 2026-10-27 engines audit.

use simorph_core::*;
use simorph_experiment::*;

/// y = NaN when a > 0.5, else a.
struct NanAbove {
    m: ComponentManifest,
}
impl NanAbove {
    fn new() -> Self {
        Self {
            m: ComponentManifest::in_process("nan-above", "1", "t")
                .with_inputs(vec![Port::scalar("a", "1")])
                .with_outputs(vec![Port::scalar("y", "1")]),
        }
    }
}
impl Component for NanAbove {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let a = req.get_f64("a")?;
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("y", if a > 0.5 { f64::NAN } else { a }))
    }
}

struct AlwaysFails {
    m: ComponentManifest,
}
impl Component for AlwaysFails {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, _req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        Err(SimorphError::Numerical("always".into()))
    }
}

#[test]
fn audit_nan_runs_are_failures_and_criteria_fail() {
    // Audit probe: 5 of 11 runs returned NaN, counted as Success, and the
    // criterion max(y) < 0.6 was met.
    let c = NanAbove::new();
    let design =
        Design::full_factorial(vec![Factor::new("a", "1", 0.0, 1.0).with_levels(11)]).unwrap();
    let plan = ExperimentPlan::new("nan", c.m.id, design, 1).with_criterion(SuccessCriterion {
        output: "y".into(),
        statistic: "max".into(),
        comparator: "<".into(),
        threshold: 0.6,
    })
    .with_criterion(SuccessCriterion {
        output: "y".into(),
        statistic: "fraction_ok".into(),
        comparator: ">=".into(),
        threshold: 1.0,
    });
    let r = run_experiment(&plan, &c, &ExecutionControl::default()).unwrap();
    assert_eq!(r.n_failed(), 5);
    assert_eq!(r.summaries["y"].count, 6);
    assert!(!r.all_criteria_met());
    assert!(r.runs.iter().any(|x| matches!(&x.status, RunStatus::Failed { error } if error.contains("non-finite output `y`"))));
}

#[test]
fn audit_no_criteria_is_not_success() {
    let c = NanAbove::new();
    let design =
        Design::full_factorial(vec![Factor::new("a", "1", 0.0, 0.4).with_levels(3)]).unwrap();
    let r = run_experiment(
        &ExperimentPlan::new("none", c.m.id, design, 1),
        &c,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(!r.criteria_defined());
    assert!(!r.all_criteria_met());
}

#[test]
fn audit_counter_example_search_reports_failures() {
    // Audit probe: a component that always fails returned Ok(None).
    let c = AlwaysFails {
        m: ComponentManifest::in_process("fails", "1", "t")
            .with_inputs(vec![Port::scalar("a", "1")])
            .with_outputs(vec![Port::scalar("y", "1")]),
    };
    let f = vec![Factor::new("a", "1", 0.0, 1.0)];
    let r = find_counter_example(
        &c,
        f.clone(),
        &Default::default(),
        |_| true,
        20,
        5,
        3,
        &ExecutionControl::default(),
    );
    assert!(r.is_err());
    let s = search_counter_example(
        &c,
        f,
        &Default::default(),
        |_| true,
        20,
        5,
        3,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!((s.evaluations, s.errors), (20, 20));
}

/// Output shape changes with a global switch (simulates a changed component).
struct Shape {
    m: ComponentManifest,
    n: usize,
}
impl Component for Shape {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let a = req.get_f64("a")?;
        let v: Vec<f64> = (0..self.n).map(|i| if i < 2 { a } else { 0.0 }).collect();
        Ok(TaskResult::new(req, EvidenceLevel::Computed).output("v", serde_json::json!(v)))
    }
}

#[test]
fn audit_reproduce_detects_shape_changes() {
    let m = ComponentManifest::in_process("shape", "1", "t")
        .with_inputs(vec![Port::scalar("a", "1")])
        .with_outputs(vec![Port {
            name: "v".into(),
            unit: "1".into(),
            description: None,
            shape: vec![0],
            range: None,
        }]);
    let c2 = Shape { m: m.clone(), n: 2 };
    let c3 = Shape { m, n: 3 };
    let design =
        Design::full_factorial(vec![Factor::new("a", "1", 0.0, 1.0).with_levels(3)]).unwrap();
    let rec = run_experiment(
        &ExperimentPlan::new("s", c2.m.id, design, 1),
        &c2,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(reproduce(&rec, &c2, &ExecutionControl::default())
        .unwrap()
        .bitwise_identical());
    let r = reproduce(&rec, &c3, &ExecutionControl::default()).unwrap();
    assert_eq!(r.structure_mismatches.len(), 3);
    assert!(!r.within(1.0));
}
