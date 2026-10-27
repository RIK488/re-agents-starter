//! Regression tests for defects found by the engine audit.

use simorph_core::{ExecutionControl, SimorphError, Uncertainty};
use simorph_uncertainty::*;

fn ctl() -> ExecutionControl {
    ExecutionControl::default()
}

#[test]
fn audit_sobol_design_depends_on_seed_and_keeps_net_structure() {
    let a = unit_design(Design::Sobol, 64, 3, 1);
    let b = unit_design(Design::Sobol, 64, 3, 2);
    assert_ne!(a, b, "seed was ignored by the Sobol design");
    assert_eq!(a, unit_design(Design::Sobol, 64, 3, 1), "not reproducible");
    // unscrambled option is still the plain sequence
    assert_eq!(unit_design(Design::SobolUnscrambled, 16, 2, 9), sobol(16, 2, true));
    // a digital shift preserves stratification: with 64 = 2^6 points (plus the
    // skipped origin, use the unskipped set) every 1/64 slice of every axis
    // holds exactly one point
    let p = sobol_scrambled(64, 3, false, 5);
    for j in 0..3 {
        let mut seen = vec![0; 64];
        for x in &p {
            assert!((0.0..1.0).contains(&x[j]));
            seen[(x[j] * 64.0) as usize] += 1;
        }
        assert!(seen.iter().all(|&c| c == 1), "dimension {j}: {seen:?}");
    }
}

#[test]
fn audit_sobol_indices_refuse_tiny_samples_and_report_warnings() {
    let inputs = vec![InputSpec::uniform("a", 0.0, 1.0), InputSpec::uniform("b", 0.0, 1.0)];
    let f = |x: &[f64]| Ok(vec![x[0] + 0.1 * x[1]]);
    let err = sobol_indices(&inputs, 8, 1, 0, f, &ctl()).unwrap_err();
    assert!(matches!(err, SimorphError::InvalidInput(_)), "{err}");
    let r = sobol_indices(&inputs, 100, 1, 0, f, &ctl()).unwrap();
    assert!(r.warnings.iter().any(|w| w.contains("power of two")), "{:?}", r.warnings);
    assert_eq!(r.design, "sobol-scrambled");
    let r = sobol_indices(&inputs, 128, 1, 0, f, &ctl()).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    // 9 inputs → 18 > 16 Sobol dimensions: fallback is reported
    let many: Vec<InputSpec> = (0..9).map(|i| InputSpec::uniform(&format!("x{i}"), 0.0, 1.0)).collect();
    let r = sobol_indices(&many, 64, 1, 0, |x| Ok(vec![x.iter().sum()]), &ctl()).unwrap();
    assert_eq!(r.design, "latin-hypercube");
    assert!(r.warnings.iter().any(|w| w.contains("fell back")), "{:?}", r.warnings);
}

#[test]
fn audit_interval_is_not_silently_uniform() {
    let iv = InputSpec::new("x", 0.5, Uncertainty::Interval { low: 0.0, high: 1.0 });
    let f = |x: &[f64]| Ok(vec![x[0] * x[0]]);
    for design in [Design::MonteCarlo, Design::LatinHypercube, Design::Sobol] {
        assert!(propagate(&[iv.clone()], design, 50, 1, f, &ctl()).is_err(), "{design:?}");
    }
    // explicit opt-in is accepted
    let r = propagate(&[iv.clone().interval_as_uniform()], Design::LatinHypercube, 1000, 1, f, &ctl()).unwrap();
    assert!((r.summaries[0].mean - 1.0 / 3.0).abs() < 0.01);
    // set-valued propagation: y = (x - 0.5)^2 on [0, 1] has range [0, 0.25];
    // the minimum is interior, which corners alone would miss
    let b = propagate_interval(&[iv], 200, 3, |x| Ok(vec![(x[0] - 0.5).powi(2)]), &ctl()).unwrap();
    assert!(!b.rigorous);
    assert!(b.bounds[0].0 < 1e-4 && (b.bounds[0].1 - 0.25).abs() < 1e-12, "{:?}", b.bounds);
}

#[test]
fn audit_morris_with_zero_trajectories_is_an_error() {
    let inputs = vec![InputSpec::uniform("a", 0.0, 1.0)];
    assert!(morris_screening(&inputs, 0, 4, 1, 0, |x| Ok(vec![x[0]]), &ctl()).is_err());
    assert!(morris_screening(&inputs, 3, 4, 1, 5, |x| Ok(vec![x[0]]), &ctl()).is_err(), "bad output index must not panic");
}

#[test]
fn audit_non_finite_outputs_are_failures() {
    let inputs = vec![InputSpec::uniform("x", 0.0, 1.0)];
    let r = propagate(
        &inputs,
        Design::LatinHypercube,
        100,
        1,
        |x| Ok(vec![if x[0] > 0.8 { f64::NAN } else { x[0] }]),
        &ctl(),
    )
    .unwrap();
    assert!(r.summaries[0].mean.is_finite(), "NaN entered the statistics");
    assert_eq!(r.failures, 20);
    assert_eq!(r.outputs.len(), 80);
    assert!(r.outputs.iter().all(|o| o[0] <= 0.8));
}
