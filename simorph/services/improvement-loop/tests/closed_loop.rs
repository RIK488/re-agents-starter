//! Demonstrations and controls of the closed improvement loop.

use simorph_core::*;
use simorph_provenance::{Ledger, ProvenanceKind};
use simorph_svc_improvement_loop::*;

fn ctl() -> ExecutionControl {
    ExecutionControl::default()
}

/// (i) a real improvement cycle on the pipe-flow surrogate, accepted on the sealed suite.
#[test]
fn pipe_flow_surrogate_is_improved_and_accepted() {
    let ledger = Ledger::in_memory();
    let r = run_improvement_campaign_with(
        ImprovementConfig::quick(TargetKind::PipeFlow),
        ledger.clone(),
        &ctl(),
    )
    .unwrap();
    println!("{}", r.to_markdown());
    assert!(r.accepted_versions >= 1);
    assert!(
        r.relative_rmse_gain() >= 0.30,
        "gain {}",
        r.relative_rmse_gain()
    );
    // every accepted candidate was judged on the same sealed suite
    assert!(r
        .iterations
        .iter()
        .all(|i| i.candidate.suite_hash == r.suite_hash));
    // provenance: hypotheses, experiments, datasets, models, validations, improvements; chain intact
    for k in [
        ProvenanceKind::Hypothesis,
        ProvenanceKind::Experiment,
        ProvenanceKind::Dataset,
        ProvenanceKind::Model,
        ProvenanceKind::Validation,
        ProvenanceKind::Improvement,
    ] {
        assert!(!ledger.by_kind(k).is_empty(), "{k:?} missing");
    }
    ledger.verify_chain().unwrap();
    // the improved model is still physically consistent
    assert!(
        r.final_evaluation.monotonicity_violation_rate
            <= r.initial.monotonicity_violation_rate + 0.01
    );
    assert!(serde_json::from_str::<serde_json::Value>(&r.to_json()).is_ok());
}

/// (i) bis: second simulator (ODE with event detection, verified against the elliptic integral).
#[test]
fn pendulum_surrogate_is_improved_and_accepted() {
    let r = run_improvement_campaign(ImprovementConfig::quick(TargetKind::PendulumPeriod)).unwrap();
    println!("{}", r.to_markdown());
    assert!(r.accepted_versions >= 1);
    assert!(
        r.relative_rmse_gain() >= 0.30,
        "gain {}",
        r.relative_rmse_gain()
    );
}

/// (ii) negative control: a unit bug (kPa stored as Pa) introduced after validation
/// trains "fine" but is rejected by the sealed suite; the current model is untouched.
#[test]
fn corrupted_labels_are_rejected_by_the_sealed_suite() {
    let cfg = ImprovementConfig {
        variant: Variant::CorruptedLabels { factor: 1e-3 },
        ..ImprovementConfig::quick(TargetKind::PipeFlow)
    };
    let mut st = ImprovementState::initialise(cfg, Ledger::in_memory(), &ctl()).unwrap();
    let before = st.model.content_hash();
    let rec = st.iterate(1, &ctl()).unwrap();
    println!(
        "corrupted: train loss {:.3e}, suite RMSE {:.4} → {:.4}; {:?}",
        rec.training.final_train_loss,
        st.evaluation.rmse_log10,
        rec.candidate.rmse_log10,
        rec.decision.reasons
    );
    assert!(rec.training.final_train_loss.is_finite());
    assert!(!rec.decision.accepted);
    assert!(rec.candidate.rmse_log10 > st.evaluation.rmse_log10);
    assert_eq!(
        st.model.content_hash(),
        before,
        "rejected candidate must not replace the model"
    );
}

/// (ii) bis: a buggy simulator upstream of validation — the independent
/// physical bounds (Hagen–Poiseuille in laminar flow) reject what they can,
/// and the sealed suite rejects the rest.
#[test]
fn buggy_simulator_is_caught_by_validation_and_suite() {
    let cfg = ImprovementConfig {
        variant: Variant::BuggySimulator { factor: 1e-3 },
        ..ImprovementConfig::quick(TargetKind::PipeFlow)
    };
    let mut st = ImprovementState::initialise(cfg, Ledger::in_memory(), &ctl()).unwrap();
    let rec = st.iterate(1, &ctl()).unwrap();
    println!(
        "buggy simulator: {} validated, {} rejected ({:?}); accepted = {}",
        rec.n_validated,
        rec.n_rejected,
        rec.rejection_examples.first(),
        rec.decision.accepted
    );
    // The component's own reference case (Hagen–Poiseuille, 32 Pa) fails for the
    // buggy simulator, so the whole batch is rejected before any training.
    assert_eq!(rec.n_validated, 0);
    assert_eq!(rec.n_rejected, rec.n_requested);
    assert!(rec
        .rejection_examples
        .iter()
        .all(|r| r.contains("reference cases")));
    assert!(!rec.decision.accepted);
    // Without the batch-level reference check, the per-sample physical bound still
    // catches laminar samples: verify that layer independently.
    let t = pipe_flow_target();
    let x = [0.01, 0.05, 1e-5];
    let dp = t.simulate(&x, &ctl()).unwrap();
    assert!((t.bounds)(&t.input_map(&x), dp * 1e-3)
        .unwrap_err()
        .contains("Poiseuille"));
}

/// (iii) the improver cannot modify its judge.
#[test]
fn sealed_suite_cannot_be_modified() {
    let mut st = ImprovementState::initialise(
        ImprovementConfig {
            suite_size: 40,
            probe_size: 20,
            ..ImprovementConfig::quick(TargetKind::PendulumPeriod)
        },
        Ledger::in_memory(),
        &ctl(),
    )
    .unwrap();
    let case = ReferenceCase {
        name: "easy".into(),
        inputs: serde_json::json!({"theta0": 0.1, "length": 1.0}),
        expected_outputs: serde_json::json!({"period": 2.0}),
        rtol: 0.0,
        atol: 0.0,
        source: "improver".into(),
    };
    assert!(matches!(
        st.suite.try_add(case),
        Err(SimorphError::Protected(_))
    ));
    assert!(matches!(
        st.suite.try_remove("p0000"),
        Err(SimorphError::Protected(_))
    ));
    // a suite edited on disk is detected before use
    let mut json: serde_json::Value = serde_json::to_value(&st.suite).unwrap();
    json["refs"]["cases"][0]["case"]["expected_outputs"]["period"] = serde_json::json!(1.0);
    let tampered: SealedEvaluationSuite = serde_json::from_value(json).unwrap();
    assert!(matches!(tampered.points(), Err(SimorphError::Protected(_))));
    assert!(st.evaluate(&st.model.clone()).is_ok());
}

/// (iv) targeted vs uniform sampling with the same number of extra samples.
/// The result is reported, whichever way it goes.
#[test]
fn targeted_vs_uniform_sampling_is_measured() {
    for t in [TargetKind::PipeFlow, TargetKind::PendulumPeriod] {
        let c = compare_targeted_vs_uniform(&ImprovementConfig::quick(t), &ctl()).unwrap();
        println!("{t:?}: initial {:.4}, targeted {:.4} (worst {:?}), hybrid {:.4}, uniform {:.4} (worst {:?}), +{} samples", c.initial_rmse, c.targeted_rmse, c.targeted_worst_regime, c.hybrid_rmse, c.uniform_rmse, c.uniform_worst_regime, c.samples);
        assert!(
            c.targeted_rmse.is_finite() && c.uniform_rmse.is_finite() && c.hybrid_rmse.is_finite()
        );
        assert!(c.targeted_rmse < c.initial_rmse && c.uniform_rmse < c.initial_rmse);
    }
}

/// (iv) bis: the same comparison over several seeds (long; run in release):
/// `cargo test -p simorph-svc-improvement-loop --release -- --ignored --nocapture over_seeds`
#[test]
#[ignore]
fn targeted_vs_uniform_over_seeds() {
    for t in [TargetKind::PipeFlow, TargetKind::PendulumPeriod] {
        let (mut wins, mut hwins, mut awins, mut rows) = (0, 0, 0, vec![]);
        let (mut st, mut su, mut sh, mut sa) = (0.0, 0.0, 0.0, 0.0);
        for seed in 1..=8u64 {
            let c = compare_targeted_vs_uniform(
                &ImprovementConfig {
                    seed,
                    ..ImprovementConfig::quick(t)
                },
                &ctl(),
            )
            .unwrap();
            wins += usize::from(c.targeted_rmse < c.uniform_rmse);
            hwins += usize::from(c.hybrid_rmse < c.uniform_rmse);
            st += c.targeted_rmse / 8.0;
            su += c.uniform_rmse / 8.0;
            sh += c.hybrid_rmse / 8.0;
            sa += c.adaptive_rmse / 8.0;
            awins += usize::from(c.adaptive_rmse <= c.uniform_rmse);
            rows.push((
                seed,
                c.initial_rmse,
                c.targeted_rmse,
                c.hybrid_rmse,
                c.uniform_rmse,
                c.adaptive_rmse,
            ));
        }
        println!("{t:?}: mean RMSE targeted {st:.4}, hybrid {sh:.4}, uniform {su:.4}, adaptive {sa:.4}; vs uniform: targeted better {wins}/8, hybrid {hwins}/8, adaptive ≤ {awins}/8; (seed, initial, targeted, hybrid, uniform, adaptive) = {rows:.4?}");
    }
}

/// (v) determinism and rollback.
#[test]
fn determinism_and_rollback() {
    let cfg = ImprovementConfig {
        max_iterations: 1,
        ..ImprovementConfig::quick(TargetKind::PendulumPeriod)
    };
    let a = run_improvement_campaign(cfg.clone()).unwrap();
    let b = run_improvement_campaign(cfg.clone()).unwrap();
    assert_eq!(a.final_evaluation.model_hash, b.final_evaluation.model_hash);
    assert_eq!(a.suite_hash, b.suite_hash);
    assert_eq!(a.final_evaluation.rmse_log10, b.final_evaluation.rmse_log10);
    let mut st = ImprovementState::initialise(cfg, Ledger::in_memory(), &ctl()).unwrap();
    let initial = st.model.content_hash();
    assert!(st.rollback().is_err());
    let rec = st.iterate(1, &ctl()).unwrap();
    assert!(rec.decision.accepted);
    assert_ne!(st.model.content_hash(), initial);
    st.rollback().unwrap();
    assert_eq!(st.model.content_hash(), initial);
    assert!(!st.ledger.by_kind(ProvenanceKind::Correction).is_empty());
}

/// Adaptive strategy: three candidates trained, selection on the probe set,
/// simulation cost accounted for.
#[test]
fn adaptive_strategy_selects_on_probe_and_accounts_cost() {
    let cfg = ImprovementConfig::recommended(TargetKind::PendulumPeriod);
    let mut st = ImprovementState::initialise(cfg, Ledger::in_memory(), &ctl()).unwrap();
    let rec = st.iterate(1, &ctl()).unwrap();
    println!(
        "adaptive selection: {:?} → accepted = {}",
        rec.strategy_selection, rec.decision.accepted
    );
    assert_eq!(rec.strategy_selection.len(), 3);
    assert_eq!(rec.n_simulated, 3 * rec.n_requested);
    assert_eq!(st.simulations_used, 24 + rec.n_simulated);
}
