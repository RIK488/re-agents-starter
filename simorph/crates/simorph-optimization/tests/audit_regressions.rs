//! Regression tests for defects found by the engine audit.  Each test fails
//! on the behaviour before the fix.

use simorph_core::*;
use simorph_optimization::Budget;
use simorph_optimization::*;
use simorph_uncertainty::InputSpec;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Counts every call and records every point.
struct Probe<F: Fn(&[f64]) -> f64 + Sync> {
    bounds: Vec<(f64, f64)>,
    f: F,
    calls: AtomicUsize,
    points: Mutex<Vec<Vec<f64>>>,
}
impl<F: Fn(&[f64]) -> f64 + Sync> Probe<F> {
    fn new(bounds: Vec<(f64, f64)>, f: F) -> Self {
        Self {
            bounds,
            f,
            calls: AtomicUsize::new(0),
            points: Mutex::new(vec![]),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
impl<F: Fn(&[f64]) -> f64 + Sync> ObjectiveFunction for Probe<F> {
    fn dimension(&self) -> usize {
        self.bounds.len()
    }
    fn bounds(&self) -> Vec<(f64, f64)> {
        self.bounds.clone()
    }
    fn evaluate(&self, x: &[f64]) -> Result<Evaluation> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.points.lock().unwrap().push(x.to_vec());
        Ok(Evaluation::scalar((self.f)(x)))
    }
}

fn ctl() -> ExecutionControl {
    ExecutionControl::default()
}

type Runner = fn(&dyn ObjectiveFunction, &Budget) -> Result<OptimizationResult>;

fn all_methods() -> Vec<(&'static str, Runner)> {
    vec![
        ("nelder_mead", |p, b| {
            nelder_mead(p, None, b, &ExecutionControl::default())
        }),
        ("gradient_descent", |p, b| {
            gradient_descent(p, None, b, &ExecutionControl::default())
        }),
        ("lbfgs", |p, b| {
            lbfgs(p, None, 5, b, &ExecutionControl::default())
        }),
        ("de", |p, b| {
            differential_evolution(p, &DeOptions::default(), b, &ExecutionControl::default())
        }),
        ("cma_es", |p, b| {
            cma_es(
                p,
                None,
                &CmaOptions::default(),
                b,
                &ExecutionControl::default(),
            )
        }),
        ("annealing", |p, b| {
            simulated_annealing(
                p,
                None,
                &SaOptions::default(),
                b,
                &ExecutionControl::default(),
            )
        }),
        ("bayesian", |p, b| {
            bayesian_optimization(p, &BoOptions::default(), b, &ExecutionControl::default())
        }),
        ("random", |p, b| {
            random_search(p, b, &ExecutionControl::default())
        }),
    ]
}

#[test]
fn audit_every_method_respects_the_evaluation_budget() {
    for (name, run) in all_methods() {
        for max in [7usize, 23, 61] {
            let p = Probe::new(vec![(-5.0, 5.0); 3], |x| {
                x.iter().map(|v| (v - 1.0).powi(2)).sum()
            });
            let r =
                run(&p, &Budget::new(max).seed(3)).unwrap_or_else(|e| panic!("{name}/{max}: {e}"));
            assert!(
                p.calls() <= max,
                "{name}: {} calls for a budget of {max}",
                p.calls()
            );
            assert_eq!(
                r.evaluations,
                p.calls(),
                "{name}: reported evaluations differ from real calls"
            );
            assert!(r.best_f.is_finite(), "{name}: best_f {}", r.best_f);
        }
    }
}

#[test]
fn audit_nan_objectives_are_worst_not_panics() {
    for (name, run) in all_methods() {
        // NaN on half of the box; the optimum (x = -1) is in the finite half
        let p = Probe::new(vec![(-3.0, 3.0); 2], |x| {
            if x[0] > 0.0 {
                f64::NAN
            } else {
                (x[0] + 1.0).powi(2) + x[1].powi(2)
            }
        });
        // the GP fit of BO is O(n³) per iteration: keep its budget small
        let max = if name == "bayesian" { 60 } else { 300 };
        let r = run(&p, &Budget::new(max).seed(5)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(r.best_f.is_finite(), "{name}: best_f {}", r.best_f);
        assert!(
            r.best_x[0] <= 0.0,
            "{name}: best point in the NaN region {:?}",
            r.best_x
        );
    }
}

#[test]
fn audit_random_search_with_only_nan_is_an_error() {
    let p = Probe::new(vec![(0.0, 1.0)], |_| f64::NAN);
    assert!(random_search(&p, &Budget::new(20), &ctl()).is_err());
}

#[test]
fn audit_de_budget_smaller_than_population_returns_best_so_far() {
    let p = Probe::new(vec![(-5.0, 5.0); 4], |x| x.iter().map(|v| v * v).sum());
    let opts = DeOptions {
        population: 50,
        ..Default::default()
    };
    let r = differential_evolution(&p, &opts, &Budget::new(10).seed(1), &ctl()).unwrap();
    assert_eq!(r.evaluations, 10);
    assert!(r.best_f.is_finite());
}

#[test]
fn audit_de_bound_handling_does_not_pile_points_on_bounds() {
    // Optimum at x = 2 near the lower bound 0 of a wide box.  Clamping put a
    // large share of trial points exactly on x = 0.
    let p = Probe::new(vec![(0.0, 100.0)], |x| (x[0] - 2.0).powi(2));
    let r = differential_evolution(
        &p,
        &DeOptions {
            population: 20,
            ..Default::default()
        },
        &Budget::new(2000).seed(4).tolerance(1e-14, 1e-10),
        &ctl(),
    )
    .unwrap();
    let pts = p.points.lock().unwrap();
    let on_bound = pts.iter().filter(|x| x[0] == 0.0 || x[0] == 100.0).count();
    assert_eq!(
        on_bound,
        0,
        "{on_bound} of {} points exactly on a bound",
        pts.len()
    );
    assert!((r.best_x[0] - 2.0).abs() < 1e-4, "x = {}", r.best_x[0]);
}

#[test]
fn audit_finite_differences_stay_inside_bounds() {
    // Model undefined outside [0, 1] (sqrt of a negative): the minimum is on
    // the lower bound, where central differences used to sample x < 0.
    let p = Probe::new(vec![(0.0, 1.0); 2], |x| {
        assert!(
            x.iter().all(|&v| (0.0..=1.0).contains(&v)),
            "evaluated outside the box: {x:?}"
        );
        x[0].sqrt() + (x[1] - 0.5).powi(2)
    });
    let r = lbfgs(&p, Some(&[0.3, 0.9]), 5, &Budget::new(500), &ctl()).unwrap();
    assert!(r.best_x[0] < 1e-3, "{:?}", r.best_x);
    let g = gradient_descent(&p, Some(&[0.0, 0.9]), &Budget::new(200), &ctl()).unwrap();
    assert!(g.best_f.is_finite());
}

#[test]
fn audit_hypervolume_degenerate_and_default_samples() {
    // 3 objectives, all points share f3 = reference → zero-width box
    let pts = vec![vec![0.0, 0.5, 1.0], vec![0.5, 0.0, 1.0]];
    assert_eq!(hypervolume(&pts, &[1.0, 1.0, 1.0], 1000, 1), 0.0);
    // mc_samples = 0 used to divide by zero (NaN)
    let pts = vec![vec![0.0, 0.0, 0.0]];
    let hv = hypervolume(&pts, &[1.0, 1.0, 1.0], 0, 1);
    assert!((hv - 1.0).abs() < 1e-12, "hv = {hv}");
    // NaN points are ignored, 1-D is exact
    assert_eq!(
        hypervolume(&[vec![f64::NAN, 0.0], vec![0.5, 0.5]], &[1.0, 1.0], 0, 0),
        0.25
    );
    assert_eq!(hypervolume(&[vec![0.25]], &[1.0], 0, 0), 0.75);
}

#[test]
fn audit_nsga2_respects_an_evaluation_budget() {
    let calls = AtomicUsize::new(0);
    let p = FnProblem::new(vec![(0.0, 1.0); 5], |x| {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(Evaluation {
            objectives: benchmarks::zdt1(x),
            constraints: vec![],
            cost: 0.0,
        })
    })
    .objectives(2);
    let opts = Nsga2Options {
        population: 20,
        generations: 1000,
        ..Default::default()
    };
    let r = nsga2_budgeted(&p, &opts, 1, 250, &ctl()).unwrap();
    assert!(calls.load(Ordering::SeqCst) <= 250);
    assert_eq!(r.evaluations, calls.load(Ordering::SeqCst));
    assert!(r.generations < 1000);
    assert!(nsga2_budgeted(&p, &opts, 1, 10, &ctl()).is_err());
}

#[test]
fn audit_nsga2_nan_points_do_not_enter_the_front() {
    let p = FnProblem::new(vec![(0.0, 1.0); 3], |x| {
        let mut f = benchmarks::zdt1(x);
        if x[0] > 0.5 {
            f[1] = f64::NAN;
        }
        Ok(Evaluation {
            objectives: f,
            constraints: vec![],
            cost: 0.0,
        })
    })
    .objectives(2);
    let r = nsga2(
        &p,
        &Nsga2Options {
            population: 24,
            generations: 30,
            ..Default::default()
        },
        2,
        &ctl(),
    )
    .unwrap();
    assert!(
        r.front
            .iter()
            .all(|q| q.objectives.iter().all(|v| v.is_finite())),
        "{:?}",
        r.front
    );
}

#[test]
fn audit_robust_worst_case_is_labelled_worst_case() {
    let unc = vec![InputSpec::new(
        "t",
        0.0,
        Uncertainty::Uniform {
            low: -0.1,
            high: 0.1,
        },
    )];
    let p = RobustProblem::new(
        vec![(-1.0, 1.0)],
        &unc,
        16,
        1,
        RobustCriterion::WorstCase,
        |x, t| Ok((x[0] + t[0]).powi(2)),
    )
    .unwrap();
    let r = robust_optimize(&p, &Budget::new(300).seed(1), &ctl()).unwrap();
    assert_eq!(r.method, Method::RobustWorstCase);
}

// ---------------------------------------------------------------- adapter

struct MmComponent {
    m: ComponentManifest,
}
impl Component for MmComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.m
    }
    fn execute(&self, req: &TaskRequest, _: &ExecutionControl) -> Result<TaskResult> {
        // input port in mm, outputs in mm
        let x_mm = req.get_f64("x")?;
        Ok(TaskResult::new(req, EvidenceLevel::Computed)
            .output("y", (x_mm - 3000.0).powi(2) / 1000.0) // mm (scaled)
            .output("z", x_mm))
    }
}

fn mm_component() -> MmComponent {
    MmComponent {
        m: ComponentManifest::in_process("mm", "1", "t")
            .with_inputs(vec![Port::scalar("x", "mm")])
            .with_outputs(vec![Port::scalar("y", "mm"), Port::scalar("z", "mm")]),
    }
}

fn spec_in_metres(hard: bool) -> ProblemSpec {
    let mut spec = ProblemSpec::new("units", "");
    spec.design_variables.push(DesignVariable {
        name: "x".into(),
        unit: "m".into(),
        low: 0.0,
        high: 10.0,
        integer: false,
        initial: None,
    });
    spec.objectives.push(Objective {
        name: "y".into(),
        direction: Direction::Minimize,
        variable: "y".into(),
        unit: "m".into(),
        target: None,
        weight: 1.0,
    });
    spec.constraints.push(Constraint {
        name: "z ≤ 2 m".into(),
        variable: "z".into(),
        kind: ConstraintKind::UpperBound,
        bound: 2.0,
        unit: "m".into(),
        tolerance: 0.0,
        hard,
    });
    spec
}

#[test]
fn audit_adapter_converts_units_between_spec_and_ports() {
    let comp = mm_component();
    let spec = spec_in_metres(true);
    let p = ComponentProblem::new(&comp, &spec, TaskRequest::new(comp.m.id), ctl()).unwrap();
    // x = 2.5 m is sent as 2500 mm; z = 2500 mm = 2.5 m violates z ≤ 2 m by 0.5 m
    let e = p.evaluate(&[2.5]).unwrap();
    assert!(
        (e.constraints[0] - 0.5).abs() < 1e-12,
        "{:?}",
        e.constraints
    );
    // y = (2500-3000)²/1000 mm = 250 mm = 0.25 m
    assert!((e.objectives[0] - 0.25).abs() < 1e-12, "{:?}", e.objectives);
    let r = differential_evolution(
        &p,
        &Default::default(),
        &Budget::new(1500).seed(2).tolerance(1e-14, 1e-10),
        &ctl(),
    )
    .unwrap();
    assert!((r.best_x[0] - 2.0).abs() < 1e-3, "x = {} m", r.best_x[0]);
    // incompatible units are refused at construction
    let mut bad = spec_in_metres(true);
    bad.constraints[0].unit = "s".into();
    assert!(ComponentProblem::new(&comp, &bad, TaskRequest::new(comp.m.id), ctl()).is_err());
}

#[test]
fn audit_adapter_soft_constraints_and_weights() {
    let comp = mm_component();
    let mut spec = spec_in_metres(false);
    spec.objectives[0].weight = 3.0;
    let p = ComponentProblem::new(&comp, &spec, TaskRequest::new(comp.m.id), ctl()).unwrap();
    assert_eq!(
        p.n_constraints(),
        0,
        "soft constraint must not be a hard constraint"
    );
    let e = p.evaluate(&[1.0]).unwrap(); // feasible: z = 1 m
                                         // weight applied: y = (1000-3000)²/1000 mm = 4 m, × 3
    assert!((e.objectives[0] - 12.0).abs() < 1e-9, "{:?}", e.objectives);
    let e = p.evaluate(&[2.5]).unwrap(); // violates by 0.5 m → penalty 1e3 · 0.25
    assert!(
        (e.objectives[0] - (3.0 * 0.25 + 250.0)).abs() < 1e-9,
        "{:?}",
        e.objectives
    );
    spec.objectives[0].weight = -1.0;
    assert!(ComponentProblem::new(&comp, &spec, TaskRequest::new(comp.m.id), ctl()).is_err());
}
