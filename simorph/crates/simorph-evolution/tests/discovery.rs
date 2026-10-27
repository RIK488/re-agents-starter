//! Scientific-discovery demonstrations (subsystem 10, "résoudre des
//! problèmes nouveaux").  Data come from integrated simulations; analytic
//! laws are used only as independent references for judging the result.

use simorph_core::*;
use simorph_evolution::discovery::*;
use simorph_evolution::*;
use simorph_provenance::{Ledger, ProvenanceKind};

fn kepler_ref(x: &[f64]) -> f64 {
    2.0 * std::f64::consts::PI * (x[0].powi(3) / x[1]).sqrt()
}

fn kepler_data(ctl: &ExecutionControl) -> (Dataset, Dataset) {
    let train = kepler_dataset(&["Earth", "Moon", "Mars", "Jupiter"], 8, 1e-4, 11, ctl).unwrap();
    // held-out: bodies never seen during the search (generalisation across μ)
    let held = kepler_dataset(&["Venus", "Sun"], 8, 1e-4, 12, ctl).unwrap();
    (train, held)
}

#[test]
fn kepler_third_law_rediscovered_from_integrated_orbits() {
    let ctl = ExecutionControl::default();
    let (train, held) = kepler_data(&ctl);
    let cfg = physics_config(3, DimensionMode::Reject, 3);
    let ledger = Ledger::in_memory();
    let d = discover(
        "Kepler",
        &train,
        &held,
        &cfg,
        0.5,
        1e-3,
        Some(("2π√(a³/μ)", &kepler_ref, 1e-3)),
        &ledger,
        &ctl,
    )
    .unwrap();
    println!(
        "Kepler: {} | level {:?} | gens {} evals {} | {:.2} s",
        d.claim.statement, d.claim.level, d.gp.generations, d.gp.evaluations, d.gp.wall_time_s
    );
    let LawVerdict::Law {
        expr,
        held_out_error,
        dimension_ok,
        ..
    } = &d.verdict
    else {
        panic!("no law: {:?}", d.verdict)
    };
    assert!(*dimension_ok);
    assert!(*held_out_error < 1e-3, "{held_out_error}");
    for x in &held.x {
        assert!(((expr.eval(x) - kepler_ref(x)) / kepler_ref(x)).abs() < 1e-3);
    }
    // the distractor (eccentricity) must not be needed
    assert_eq!(
        expr.var_mask() & 0b100,
        0,
        "law uses e: {}",
        d.claim.statement
    );
    assert_eq!(d.claim.level, EvidenceLevel::Simulated);
    assert!(d.validation.as_ref().unwrap().all_passed());
    ledger.verify_chain().unwrap();
    assert!(!ledger.by_kind(ProvenanceKind::Run).is_empty());
    assert_eq!(ledger.by_kind(ProvenanceKind::Decision).len(), 1);
}

#[test]
fn pendulum_law_and_amplitude_correction_rediscovered() {
    let ctl = ExecutionControl::default();
    let ledger = Ledger::in_memory();
    // stage 1: small amplitude
    let small = pendulum_dataset(40, (0.005, 0.02), 1e-5, 21, &ctl).unwrap();
    let small_held = pendulum_dataset(15, (0.005, 0.02), 1e-5, 22, &ctl).unwrap();
    let cfg = physics_config(3, DimensionMode::Reject, 5);
    let small_ref = |x: &[f64]| 2.0 * std::f64::consts::PI * (x[0] / x[1]).sqrt();
    let d1 = discover(
        "Pendule petites oscillations",
        &small,
        &small_held,
        &cfg,
        0.5,
        1e-3,
        Some(("2π√(L/g)", &small_ref, 2e-4)),
        &ledger,
        &ctl,
    )
    .unwrap();
    println!(
        "pendulum stage 1: {} ({:?})",
        d1.claim.statement, d1.claim.level
    );
    let LawVerdict::Law { expr: law0, .. } = &d1.verdict else {
        panic!("no small-angle law: {:?}", d1.verdict)
    };
    assert_eq!(
        law0.var_mask() & 0b100,
        0,
        "small-angle law should not depend on θ0"
    );
    // stage 2: ratio T / law0(L, g) as a function of θ0 (uses the DISCOVERED law)
    let big = pendulum_dataset(60, (0.05, 1.2), 1e-6, 23, &ctl).unwrap();
    let big_held = pendulum_dataset(20, (0.05, 1.2), 1e-6, 24, &ctl).unwrap();
    let ratio = |d: &Dataset| {
        let mut r = Dataset::new(&["theta0"], &["1"], "T_over_T0", "1");
        for (x, y) in d.x.iter().zip(&d.y) {
            r.push(vec![x[2]], y / law0.eval(x));
        }
        r
    };
    let (rt, rh) = (ratio(&big), ratio(&big_held));
    let mut cfg2 = physics_config(1, DimensionMode::Reject, 9);
    cfg2.target_error = 1e-6;
    cfg2.generations = 80;
    let d2 = discover(
        "Pendule correction d'amplitude",
        &rt,
        &rh,
        &cfg2,
        0.5,
        1e-3,
        None,
        &ledger,
        &ctl,
    )
    .unwrap();
    let LawVerdict::Law { expr: corr, .. } = &d2.verdict else {
        panic!("no correction law: {:?}", d2.verdict)
    };
    let c = quadratic_coefficient(corr, 0, 1, 1e-3);
    println!(
        "pendulum stage 2: {} | quadratic coefficient {c:.5} (theory 1/16 = 0.0625)",
        d2.claim.statement
    );
    assert!(
        (c - 1.0 / 16.0).abs() / (1.0 / 16.0) < 0.10,
        "coefficient {c}"
    );
    ledger.verify_chain().unwrap();
}

#[test]
fn pure_noise_yields_no_law() {
    let ctl = ExecutionControl::default();
    let all = noise_dataset(160, 31);
    let (train, held) = all.split(0.4, 32);
    let mut cfg = GpConfig::new(PrimitiveSet::full(2), 4);
    cfg.metric = ErrorMetric::NormalisedRmse;
    cfg.scaling = Scaling::Affine;
    cfg.generations = 20;
    let ledger = Ledger::in_memory();
    let d = discover(
        "Contrôle bruit",
        &train,
        &held,
        &cfg,
        0.9,
        0.5,
        None,
        &ledger,
        &ctl,
    )
    .unwrap();
    println!(
        "noise control: train error {:.3} | verdict {:?}",
        d.gp.best.error, d.verdict
    );
    assert!(
        matches!(d.verdict, LawVerdict::NoLaw { .. }),
        "{:?}",
        d.verdict
    );
    assert_eq!(d.claim.level, EvidenceLevel::UnverifiedHypothesis);
}

/// Dimension-aware vs plain GP on the Kepler data.  Release run:
/// `cargo test --release -p simorph-evolution --test discovery -- --ignored --nocapture`
#[test]
#[ignore]
fn compare_dimension_modes_on_kepler() {
    let ctl = ExecutionControl::default();
    let (train, held) = kepler_data(&ctl);
    let mut base = physics_config(3, DimensionMode::Ignore, 0);
    base.generations = 30;
    let modes = vec![
        ("plain".to_string(), DimensionMode::Ignore),
        ("penalise(0.1)".to_string(), DimensionMode::Penalise(0.1)),
        ("penalise(1)".to_string(), DimensionMode::Penalise(1.0)),
        ("penalise(10)".to_string(), DimensionMode::Penalise(10.0)),
        ("reject".to_string(), DimensionMode::Reject),
    ];
    let mut b2 = base.clone();
    b2.variation_attempts = std::env::var("VA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let rows = compare_modes(&train, &held, &modes, 20, &b2, 1e-3, &ctl).unwrap();
    for r in rows {
        println!(
            "{:<12} success {}/{} | mean evals {:.0} | median held-out rel. error {:.3e}",
            r.mode, r.successes, r.runs, r.mean_evaluations_to_stop, r.median_held_out_error
        );
    }
}
