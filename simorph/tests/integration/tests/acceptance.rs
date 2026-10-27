//! Acceptance tests referenced by `docs/STATUS.md`.

use simorph_cli::*;
use simorph_core::*;
use simorph_provenance::ProvenanceKind;
use simorph_svc_scientific_director::StopReason;

fn ws(name: &str) -> Workspace {
    let dir = std::env::temp_dir().join(format!("simorph-acc-{name}-{}", RunId::new()));
    Workspace::open(dir).unwrap()
}

/// AC-E2E-1: nanosatellite multidisciplinary campaign.
#[test]
fn ac_e2e_1_nanosat_campaign() {
    let w = ws("nanosat");
    let c = run_campaign(&w, examples::nanosat(), &ExecutionControl::default()).unwrap();
    assert_eq!(
        c.stop_reason,
        Some(StopReason::ProgrammeCompleted),
        "{}",
        c.report_markdown()
    );
    assert!(
        c.outcomes.iter().all(|o| o.succeeded()),
        "{}",
        c.report_markdown()
    );
    // eclipse fraction vs analytic cylindrical shadow
    let expected = (6_378_137.0f64 / (6_378_137.0 + 500e3)).asin() / std::f64::consts::PI;
    let ecl = c
        .conclusions
        .iter()
        .find(|k| k.variable == "eclipse_fraction")
        .unwrap();
    assert!(
        (ecl.value - expected).abs() < 0.02,
        "{} vs {expected}",
        ecl.value
    );
    assert_eq!(ecl.level, EvidenceLevel::Simulated);
    assert!(!ecl.validations.is_empty() && !ecl.supporting_runs.is_empty());
    // The retained design is the lightest feasible power subsystem: it
    // respects every hard constraint and is not saturated (DoD active or
    // energy margin active, i.e. the optimum sits on a constraint).
    let best = c.conclusions.iter().find(|k| k.variable == "power_system_mass").unwrap();
    let run = c.outcomes.iter().flat_map(|o| o.runs.iter()).find(|r| r.run_id == best.supporting_runs[0]).unwrap();
    assert!(run.outputs["energy_margin_wh"] >= 0.0, "{:?}", run.outputs);
    assert!(run.outputs["depth_of_discharge"] <= 0.3 + 1e-9, "{:?}", run.outputs);
    assert!(run.outputs["energy_balance_abs_error"] <= 0.05);
    assert!(run.outputs["final_soc"] < 0.999, "objective must not be saturated: {:?}", run.outputs);
    // Every unknown is read on the retained design.
    for u in ["eclipse_fraction", "min_soc", "max_array_temperature"] {
        let k = c.conclusions.iter().find(|k| k.variable == u).unwrap();
        assert_eq!(k.supporting_runs, best.supporting_runs, "{u} must come from the retained design");
    }
    // provenance complete and intact; validations recorded; report exported
    w.ledger.verify_chain().unwrap();
    assert!(w.ledger.by_kind(ProvenanceKind::Validation).len() >= c.runs_used as usize);
    assert!(w
        .root
        .join("campaigns")
        .join(c.id.to_string())
        .join("report.md")
        .exists());
    // memory ingested
    assert!(memory_report(&w).unwrap().contains("power_system_mass"));
}

/// Cross-sector: the same director handles civil engineering, hydraulics and finance.
#[test]
fn cross_sector_campaigns_complete_with_unit_conversions() {
    let w = ws("sectors");
    let beam = run_campaign(&w, examples::beam(), &ExecutionControl::default()).unwrap();
    assert_eq!(
        beam.stop_reason,
        Some(StopReason::ProgrammeCompleted),
        "{}",
        beam.report_markdown()
    );
    let d = beam
        .conclusions
        .iter()
        .find(|k| k.variable == "tip_deflection")
        .unwrap();
    let expected = 1000.0 / (3.0 * 2.1e11 * 2e-5);
    assert!((d.value - expected).abs() / expected < 0.02);
    let pipe = run_campaign(&w, examples::pipe(), &ExecutionControl::default()).unwrap();
    assert_eq!(
        pipe.stop_reason,
        Some(StopReason::ProgrammeCompleted),
        "{}",
        pipe.report_markdown()
    );
    let dp = pipe
        .conclusions
        .iter()
        .find(|k| k.variable == "pressure_drop")
        .unwrap();
    assert_eq!(dp.unit, "bar");
    assert!(dp.value > 0.5 && dp.value < 5.0, "{}", dp.value);
    // finance: only unknowns, no design variables → analysis says data required (cash flows missing)
    let fin = analyse(&examples::finance()).unwrap();
    let by = fin.analysis.by_category();
    assert!(by
        .get(&simorph_svc_scientific_director::QuantityCategory::RequiresData)
        .map(|v| v.contains(&"npv".to_string()))
        .unwrap_or(false));
}

/// AC-EXP-2: reproduction by id from the archive.
#[test]
fn ac_exp_2_reproduce_archived_experiment() {
    let w = ws("repro");
    run_campaign(&w, examples::beam(), &ExecutionControl::default()).unwrap();
    let ids = w.archive.list().unwrap();
    assert!(!ids.is_empty());
    let r = reproduce_experiment(&w, &ids[0].to_string(), &ExecutionControl::default()).unwrap();
    assert!(r.bitwise_identical);
}

/// AC-PROV-2: explain reconstructs the causal chain for a validation record.
#[test]
fn ac_prov_2_explain_chain() {
    let w = ws("explain");
    let mut inputs = indexmap::IndexMap::new();
    inputs.insert("arrival_rate".to_string(), serde_json::json!(1.0));
    inputs.insert("service_rate".to_string(), serde_json::json!(2.0));
    execute_component(
        Some(&w),
        "operations.queue.mm1",
        inputs,
        1.0,
        None,
        &ExecutionControl::default(),
    )
    .unwrap();
    let v = w.ledger.by_kind(ProvenanceKind::Validation).pop().unwrap();
    let md = provenance_explain(&w, &v.id.to_string()).unwrap();
    assert!(md.contains("run") || md.contains("Run"));
}

/// AC-MOD: model assembly of the nanosat problem reports descriptors with units.
#[test]
fn ac_mod_assembly_has_descriptors_and_dimensional_report() {
    let a = assemble_model(&examples::nanosat()).unwrap();
    assert!(!a.assembly.descriptors.is_empty());
    assert!(
        a.assembly.dimensional_report.iter().all(|c| c.passed),
        "{:?}",
        a.assembly.dimensional_report
    );
}

/// AC-AIL-3: a faster but worse reasoner is rejected (via the standard suite).
#[test]
fn ac_ail_3_speed_is_not_reasoning() {
    let out = lab::run_lab(None, "reasoner", 1, &ExecutionControl::default()).unwrap();
    let shallower = out
        .iterations
        .iter()
        .find(|i| i.candidate.name.contains("max_depth=1"))
        .unwrap();
    assert!(!shallower.decision.accepted());
    assert!(out.iterations.iter().any(|i| i.decision.accepted()));
}
