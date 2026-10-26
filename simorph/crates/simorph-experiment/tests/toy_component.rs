//! Integration tests on a toy component: `y = a^2 + b`, which fails
//! (returns an error) when `a > 0.8` and violates `y < 1.5` for large `a`.

use indexmap::IndexMap;
use simorph_core::*;
use simorph_experiment::*;
use simorph_provenance::{Ledger, ProvenanceKind};

struct Quad {
    m: ComponentManifest,
}

impl Quad {
    fn new() -> Self {
        Self {
            m: ComponentManifest::in_process("quad", "1", "test")
                .with_inputs(vec![
                    Port::scalar("a", "1"),
                    Port::scalar("b", "1"),
                    Port::scalar("c", "1"),
                ])
                .with_outputs(vec![Port::scalar("y", "1")]),
        }
    }
}

impl Component for Quad {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        ctl.check()?;
        let a = req.get_f64("a")?;
        let b = req.get_f64("b")?;
        if a > 0.8 {
            return Err(SimorphError::Numerical("diverged".into()));
        }
        Ok(TaskResult::new(req, EvidenceLevel::Computed).output("y", a * a + b))
    }
}

fn factors() -> Vec<Factor> {
    vec![
        Factor::new("a", "1", 0.0, 1.0).with_levels(5),
        Factor::new("b", "1", 0.0, 1.0).with_levels(5),
    ]
}

#[test]
fn run_is_deterministic_parallel_and_archived() {
    let c = Quad::new();
    let plan = ExperimentPlan::new(
        "quad-ff",
        c.m.id,
        Design::full_factorial(factors()).unwrap(),
        42,
    )
    .fixed("c", 0.0)
    .with_criterion(SuccessCriterion {
        output: "y".into(),
        statistic: "max".into(),
        comparator: "<".into(),
        threshold: 2.0,
    })
    .with_criterion(SuccessCriterion {
        output: "y".into(),
        statistic: "fraction_ok".into(),
        comparator: ">=".into(),
        threshold: 0.9,
    });
    let ctl = ExecutionControl::default();
    let r1 = run_experiment(&plan, &c, &ctl).unwrap();
    let r2 = run_experiment(&plan, &c, &ctl).unwrap();
    assert_eq!(r1.runs.len(), 25);
    assert_eq!(r1.n_failed(), 5); // a = 1.0 row
    assert_eq!(r1.outputs_hash, r2.outputs_hash);
    assert_eq!(r1.plan_hash, r2.plan_hash);
    assert!((r1.summaries["y"].max - (0.75f64.powi(2) + 1.0)).abs() < 1e-12);
    assert_eq!(r1.criteria_results[0].1, true);
    assert_eq!(r1.criteria_results[1].1, false); // 20/25 = 0.8 < 0.9
    assert!(!r1.all_criteria_met());

    // reproduce
    let rep = reproduce(&r1, &c, &ctl).unwrap();
    assert!(rep.bitwise_identical());
    assert!(rep.component_hash_matches);
    assert_eq!(rep.max_relative_deviation, 0.0);

    // archive + provenance
    let dir = tempfile::tempdir().unwrap();
    let archive = ExperimentArchive::open(dir.path()).unwrap();
    archive.store(&r1).unwrap();
    archive.store(&r1).unwrap(); // idempotent
    assert_eq!(archive.list().unwrap(), vec![r1.id]);
    let back = archive.get(r1.id).unwrap();
    assert_eq!(back.outputs_hash, r1.outputs_hash);
    let mut tampered = r1.clone();
    tampered.runs.pop();
    assert!(archive.store(&tampered).is_err());

    let ledger = Ledger::in_memory();
    let problem = ledger
        .append(ProvenanceKind::Problem, "p", serde_json::json!({}), &[])
        .unwrap();
    let exp = ledger.record_experiment(&r1, &[problem]).unwrap();
    assert_eq!(ledger.by_kind(ProvenanceKind::Run).len(), 20);
    assert_eq!(ledger.by_kind(ProvenanceKind::Failure).len(), 5);
    let ex = ledger.explain(exp).unwrap();
    assert_eq!(ex.steps[0].record.kind, ProvenanceKind::Problem);
    assert!(ledger.verify_chain().is_ok());
}

#[test]
fn cancellation_marks_runs_cancelled() {
    let c = Quad::new();
    let plan = ExperimentPlan::new("x", c.m.id, Design::full_factorial(factors()).unwrap(), 1)
        .fixed("c", 0.0);
    let ctl = ExecutionControl::default();
    ctl.cancel.cancel();
    let r = run_experiment(&plan, &c, &ctl).unwrap();
    assert!(r
        .runs
        .iter()
        .all(|x| matches!(x.status, RunStatus::Cancelled)));
}

#[test]
fn counter_example_search_finds_violation() {
    let c = Quad::new();
    let fixed: IndexMap<String, serde_json::Value> =
        [("c".to_string(), serde_json::Value::from(0.0))]
            .into_iter()
            .collect();
    let fs = vec![
        Factor::new("a", "1", 0.0, 0.8),
        Factor::new("b", "1", 0.0, 1.0),
    ];
    let ce = find_counter_example(
        &c,
        fs.clone(),
        &fixed,
        |r| r.get_f64("y").unwrap() < 1.5,
        500,
        16,
        3,
        &ExecutionControl::default(),
    )
    .unwrap()
    .expect("a violation exists: a=0.8,b=1 gives 1.64");
    assert!(ce.result.get_f64("y").unwrap() >= 1.5);
    // no violation possible when a <= 0.5 and b <= 0.5
    let fs2 = vec![
        Factor::new("a", "1", 0.0, 0.5),
        Factor::new("b", "1", 0.0, 0.5),
    ];
    let none = find_counter_example(
        &c,
        fs2,
        &fixed,
        |r| r.get_f64("y").unwrap() < 1.5,
        200,
        16,
        3,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(none.is_none());
}

#[test]
fn failure_cause_ranks_responsible_factor_first() {
    let c = Quad::new();
    let plan = ExperimentPlan::new(
        "lhs",
        c.m.id,
        Design::latin_hypercube(factors(), 60, 9).unwrap(),
        5,
    )
    .fixed("c", 0.0);
    let r = run_experiment(&plan, &c, &ExecutionControl::default()).unwrap();
    assert!(r.n_failed() > 5);
    let causes = rank_failure_causes(&r, |_| false);
    assert_eq!(causes[0].factor, "a");
    assert!(causes[0].score > causes[1].score * 2.0, "{causes:?}");
    assert!(causes[0].rank_correlation > 0.5);
    // violation predicate on y: large b drives y > 1.3 as well
    let causes2 = rank_failure_causes(&r, |run| {
        run.output_f64("y").map(|y| y > 1.3).unwrap_or(false)
    });
    assert!(causes2.iter().all(|c| c.n_bad + c.n_good == 60));
}
