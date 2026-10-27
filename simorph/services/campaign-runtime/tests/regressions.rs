//! Regression tests for defects found by the 2026-10-27 audit.

use simorph_core::*;
use simorph_plugin_engineering as eng;
use simorph_plugin_sdk::ComponentSet;
use simorph_provenance::Ledger;
use simorph_svc_campaign_runtime::run_campaign;
use simorph_svc_scientific_director::StopReason;
use std::sync::Arc;

fn set() -> ComponentSet {
    let s = ComponentSet::new();
    s.register(|| Ok(Arc::new(eng::beam::Cantilever::new()) as Arc<dyn Component>))
        .unwrap();
    s.register(|| Ok(Arc::new(eng::pipe_flow::PipeFlow::new()) as Arc<dyn Component>))
        .unwrap();
    s
}

fn beam_problem() -> ProblemSpec {
    let mut p = ProblemSpec::new(
        "poutre, moment plafonné",
        "maximiser la flèche sous moment maximal 2 kN·m",
    );
    p.knowns.push(Quantity::new("load", 1.0, "kN"));
    p.knowns.push(Quantity::new("youngs_modulus", 210.0, "GPa"));
    p.design_variables.push(DesignVariable {
        name: "length".into(),
        unit: "mm".into(),
        low: 1000.0,
        high: 3000.0,
        integer: false,
        initial: None,
    });
    p.design_variables.push(DesignVariable {
        name: "second_moment".into(),
        unit: "cm^4".into(),
        low: 500.0,
        high: 2000.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "flèche".into(),
        direction: Direction::Maximize,
        variable: "tip_deflection".into(),
        unit: "m".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "moment".into(),
        variable: "max_bending_moment".into(),
        kind: ConstraintKind::UpperBound,
        bound: 2.0,
        unit: "kN*m".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.budget.max_runs = 2000;
    p
}

/// Constraint bounds declared in kN·m must reach the optimiser in N·m, and
/// the conclusion must respect the cap.  Optimum: L = 2 m (M = P·L = 2 kN·m),
/// I = 500 cm⁴ → δ = P L³ / (3 E I) = 1000·8 / (3·2.1e11·5e-6) ≈ 2.54 mm.
#[test]
fn constraint_units_reach_optimiser_and_conclusion_is_feasible() {
    let c = run_campaign(
        beam_problem(),
        set(),
        Ledger::in_memory(),
        &ExecutionControl::default(),
    )
    .unwrap();
    let d = c
        .conclusions
        .iter()
        .find(|k| k.variable == "tip_deflection")
        .unwrap();
    let expected = 1000.0 * 8.0 / (3.0 * 2.1e11 * 5e-6);
    let run = c
        .outcomes
        .iter()
        .flat_map(|o| o.runs.iter())
        .find(|r| r.run_id == d.supporting_runs[0])
        .unwrap();
    assert!(
        run.outputs["max_bending_moment"] <= 2.0 + 1e-9,
        "conclusion violates the cap: {:?}",
        run.outputs
    );
    assert!(
        (d.value - expected).abs() / expected < 0.05,
        "δ = {} vs {expected}\n{}",
        d.value,
        c.report_markdown()
    );
}

/// A hard constraint on a secondary output (Reynolds number) must filter the
/// conclusion: the retained design is the best *feasible* one.
#[test]
fn conclusions_respect_hard_constraints() {
    let mut p = ProblemSpec::new(
        "conduite, Re plafonné",
        "minimiser la perte de charge avec Re ≤ 5e5",
    );
    for (n, v, u) in [
        ("length", 2.0, "km"),
        ("velocity", 2.5, "m/s"),
        ("density", 1000.0, "kg/m^3"),
        ("viscosity", 1.0, "mPa*s"),
        ("roughness", 0.05, "mm"),
    ] {
        p.knowns.push(Quantity::new(n, v, u));
    }
    p.design_variables.push(DesignVariable {
        name: "diameter".into(),
        unit: "mm".into(),
        low: 100.0,
        high: 600.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "Δp".into(),
        direction: Direction::Minimize,
        variable: "pressure_drop".into(),
        unit: "bar".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "Re".into(),
        variable: "reynolds".into(),
        kind: ConstraintKind::UpperBound,
        bound: 5e5,
        unit: "1".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.budget.max_runs = 500;
    let c = run_campaign(p, set(), Ledger::in_memory(), &ExecutionControl::default()).unwrap();
    let k = c
        .conclusions
        .iter()
        .find(|k| k.variable == "pressure_drop")
        .unwrap();
    let run = c
        .outcomes
        .iter()
        .flat_map(|o| o.runs.iter())
        .find(|r| r.run_id == k.supporting_runs[0])
        .unwrap();
    assert!(
        run.outputs["reynolds"] <= 5e5 * (1.0 + 1e-9),
        "{:?}",
        run.outputs
    );
    // Re = ρ v D / μ ≤ 5e5 ⇒ D ≤ 0.2 m: the best feasible design is at that bound.
    assert!(
        (run.inputs["diameter"] - 0.2).abs() < 0.01,
        "{:?}",
        run.inputs
    );
}

/// A problem no component can address must not report a completed programme.
#[test]
fn nothing_to_run_is_reported() {
    let mut p = ProblemSpec::new("dissipateur", "dimensionner un dissipateur");
    p.knowns.push(Quantity::new("power", 50.0, "W"));
    p.unknowns.push(Unknown {
        name: "junction_temperature".into(),
        unit: "K".into(),
        prior: None,
        description: None,
    });
    let c = run_campaign(p, set(), Ledger::in_memory(), &ExecutionControl::default()).unwrap();
    assert_eq!(c.stop_reason, Some(StopReason::NothingToRun));
}

fn pipe_with_observations(csv: &str, tag: &str) -> ProblemSpec {
    let dir = std::env::temp_dir().join(format!("simorph-obs-{tag}-{}", RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("obs.csv");
    std::fs::write(&f, csv).unwrap();
    let mut p = ProblemSpec::new("conduite mesurée", "comparer le modèle aux mesures");
    for (n, v, u) in [
        ("length", 10.0, "m"),
        ("density", 1000.0, "kg/m^3"),
        ("viscosity", 1.0, "mPa*s"),
        ("roughness", 0.0, "mm"),
        ("diameter", 10.0, "mm"),
    ] {
        p.knowns.push(Quantity::new(n, v, u));
    }
    p.design_variables.push(DesignVariable {
        name: "velocity".into(),
        unit: "m/s".into(),
        low: 0.01,
        high: 0.2,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "Δp".into(),
        direction: Direction::Minimize,
        variable: "pressure_drop".into(),
        unit: "Pa".into(),
        target: None,
        weight: 1.0,
    });
    p.attachments.push(Attachment {
        name: "mesures".into(),
        media_type: "text/csv".into(),
        location: f.to_string_lossy().into_owned(),
        sha256: None,
        dataset_id: None,
    });
    p.budget.max_runs = 300;
    p
}

/// Laminar Poiseuille flow, Δp = 32 μ L v / D²: observations consistent with
/// it (2 % noise, declared σ) → the calibration runs become empirically
/// validated; biased observations (+30 %) → no empirical validation.
#[test]
fn calibration_grants_empirical_validation_only_when_consistent() {
    let dp = |v: f64| 32.0 * 1e-3 * 10.0 * v / 1e-4;
    let noise = [0.01, -0.015, 0.005, -0.008, 0.012];
    let vs = [0.02, 0.05, 0.08, 0.12, 0.16];
    let mut good = String::from("velocity [m/s],pressure_drop [kPa],pressure_drop_std [kPa]\n");
    let mut bad = good.clone();
    for (v, n) in vs.iter().zip(noise) {
        let y = dp(*v);
        good.push_str(&format!(
            "{v},{},{}\n",
            y * (1.0 + n) / 1000.0,
            0.02 * y / 1000.0
        ));
        bad.push_str(&format!("{v},{},{}\n", y * 1.3 / 1000.0, 0.02 * y / 1000.0));
    }
    let empirical = |p: ProblemSpec| {
        let c = run_campaign(p, set(), Ledger::in_memory(), &ExecutionControl::default()).unwrap();
        c.outcomes
            .iter()
            .flat_map(|o| o.runs.iter())
            .filter(|r| r.evidence == EvidenceLevel::EmpiricallyValidated)
            .count()
    };
    assert_eq!(empirical(pipe_with_observations(&good, "good")), 5);
    assert_eq!(empirical(pipe_with_observations(&bad, "bad")), 0);
}
