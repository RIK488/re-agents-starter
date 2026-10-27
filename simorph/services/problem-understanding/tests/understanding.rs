//! Acceptance tests of the natural-language interpreter.

mod common;
use common::*;
use simorph_core::*;
use simorph_provenance::Ledger;
use simorph_svc_problem_understanding::*;

fn to_si(v: f64, unit: &str) -> f64 {
    let u = simorph_units::parse_unit(unit).unwrap();
    u.to_si(v)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12)
}

/// Structural equivalence of two specifications (values compared in SI).
fn assert_equivalent(parsed: &ProblemSpec, reference: &ProblemSpec) {
    assert_eq!(
        parsed.knowns.len(),
        reference.knowns.len(),
        "knowns {:?}",
        parsed.knowns
    );
    for r in &reference.knowns {
        let p = parsed
            .knowns
            .iter()
            .find(|k| k.name == r.name)
            .unwrap_or_else(|| panic!("missing known {}", r.name));
        assert!(
            close(to_si(p.value, &p.unit), to_si(r.value, &r.unit)),
            "{}: {} {} vs {} {}",
            r.name,
            p.value,
            p.unit,
            r.value,
            r.unit
        );
    }
    assert_eq!(
        parsed.design_variables.len(),
        reference.design_variables.len()
    );
    for (p, r) in parsed
        .design_variables
        .iter()
        .zip(&reference.design_variables)
    {
        assert_eq!(p.name, r.name, "design variable order");
        assert!(
            close(to_si(p.low, &p.unit), to_si(r.low, &r.unit))
                && close(to_si(p.high, &p.unit), to_si(r.high, &r.unit)),
            "{}",
            r.name
        );
    }
    assert_eq!(parsed.objectives.len(), reference.objectives.len());
    for (p, r) in parsed.objectives.iter().zip(&reference.objectives) {
        assert_eq!(
            (p.variable.as_str(), p.direction),
            (r.variable.as_str(), r.direction)
        );
    }
    assert_eq!(
        parsed.constraints.len(),
        reference.constraints.len(),
        "{:?}",
        parsed.constraints
    );
    for r in &reference.constraints {
        let p = parsed
            .constraints
            .iter()
            .find(|c| c.variable == r.variable)
            .unwrap_or_else(|| panic!("missing constraint {}", r.variable));
        assert_eq!(p.kind, r.kind, "{}", r.variable);
        assert!(
            close(to_si(p.bound, &p.unit), to_si(r.bound, &r.unit))
                || (p.bound == 0.0 && r.bound == 0.0),
            "{}: {} {} vs {} {}",
            r.variable,
            p.bound,
            p.unit,
            r.bound,
            r.unit
        );
    }
    let mut pu: Vec<&str> = parsed.unknowns.iter().map(|u| u.name.as_str()).collect();
    let mut ru: Vec<&str> = reference.unknowns.iter().map(|u| u.name.as_str()).collect();
    pu.sort();
    ru.sort();
    assert_eq!(pu, ru);
    assert_eq!(parsed.budget.max_runs, reference.budget.max_runs);
}

/// Run both campaigns and compare every conclusion in SI.
fn assert_same_conclusions(
    parsed: ProblemSpec,
    reference: ProblemSpec,
) -> Vec<(String, f64, String)> {
    let ctl = ExecutionControl::default();
    let a = simorph_svc_campaign_runtime::run_campaign(
        parsed,
        component_set(),
        Ledger::in_memory(),
        &ctl,
    )
    .unwrap();
    let b = simorph_svc_campaign_runtime::run_campaign(
        reference,
        component_set(),
        Ledger::in_memory(),
        &ctl,
    )
    .unwrap();
    assert_eq!(
        format!("{:?}", a.stop_reason),
        format!("{:?}", b.stop_reason),
        "{}",
        a.report_markdown()
    );
    assert_eq!(a.conclusions.len(), b.conclusions.len());
    let mut out = vec![];
    for cb in &b.conclusions {
        let ca = a
            .conclusions
            .iter()
            .find(|c| c.variable == cb.variable)
            .unwrap();
        if cb.value.is_nan() {
            assert!(ca.value.is_nan(), "{}", cb.variable);
            continue;
        }
        let (va, vb) = (to_si(ca.value, &ca.unit), to_si(cb.value, &cb.unit));
        assert!(
            (va - vb).abs() <= 1e-6 * vb.abs().max(1e-12),
            "{}: {} {} vs {} {}",
            cb.variable,
            ca.value,
            ca.unit,
            cb.value,
            cb.unit
        );
        assert_eq!(ca.level, cb.level);
        out.push((cb.variable.clone(), ca.value, ca.unit.clone()));
    }
    out
}

#[test]
fn french_beam_text_gives_the_reference_campaign() {
    let r = understand(BEAM_FR, &manifests());
    assert!(r.ambiguities.is_empty(), "{}", r.to_markdown());
    let reference = examples::beam();
    assert_equivalent(&r.spec, &reference);
    let concl = assert_same_conclusions(r.spec, reference);
    println!("beam: {concl:?}");
}

#[test]
fn french_pipe_text_gives_the_reference_campaign() {
    let r = understand(PIPE_FR, &manifests());
    assert!(r.ambiguities.is_empty(), "{}", r.to_markdown());
    let reference = examples::pipe();
    assert_equivalent(&r.spec, &reference);
    let concl = assert_same_conclusions(r.spec, reference);
    println!("pipe: {concl:?}");
}

#[test]
fn french_nanosat_text_gives_the_reference_campaign() {
    let r = understand(NANOSAT_FR, &manifests());
    assert!(r.ambiguities.is_empty(), "{}", r.to_markdown());
    let reference = examples::nanosat();
    assert_equivalent(&r.spec, &reference);
    // uncertainties written in the text
    let load = r
        .spec
        .knowns
        .iter()
        .find(|k| k.name == "load_power")
        .unwrap();
    assert_eq!(
        load.uncertainty,
        Uncertainty::Uniform {
            low: 12.0,
            high: 16.0
        }
    );
    let eff = r
        .spec
        .knowns
        .iter()
        .find(|k| k.name == "array_efficiency")
        .unwrap();
    assert_eq!(eff.uncertainty, Uncertainty::Normal { std_dev: 0.02 });
    let concl = assert_same_conclusions(r.spec, reference);
    println!("nanosat: {concl:?}");
}

#[test]
fn english_beam_text_gives_the_reference_campaign() {
    let r = understand(BEAM_EN, &manifests());
    assert!(r.ambiguities.is_empty(), "{}", r.to_markdown());
    let reference = examples::beam();
    assert_equivalent(&r.spec, &reference);
    assert_same_conclusions(r.spec, reference);
}

#[test]
fn unknown_unit_is_reported_not_guessed() {
    let r = understand(
        "La poutre mesure 3 furlongs de long. Minimiser la flèche.",
        &manifests(),
    );
    assert!(r.has(AmbiguityKind::UnknownUnit), "{}", r.to_markdown());
    assert!(r.spec.knowns.iter().all(|k| k.name != "length"));
    assert!(r.unsupported_phrases.iter().any(|p| p.contains("furlongs")));
}

#[test]
fn unmappable_concept_is_reported_not_guessed() {
    let r = understand(
        "La température du cœur du réacteur est de 600 K. Déterminer le flux neutronique.",
        &manifests(),
    );
    assert!(r.has(AmbiguityKind::UnmappedConcept), "{}", r.to_markdown());
    assert!(r.spec.knowns.is_empty(), "{:?}", r.spec.knowns);
    assert!(r.spec.unknowns.is_empty());
    assert!(r.has(AmbiguityKind::NoObjectiveOrUnknown));
}

#[test]
fn ambiguous_tie_is_reported() {
    // Both the battery model and the nanosat model have a battery capacity
    // port; with no other context the tie must be reported.
    let r = understand(
        "La capacité de la batterie vaut 5 Ah. Déterminer l'état de charge final.",
        &manifests(),
    );
    assert!(
        r.has(AmbiguityKind::MultipleCandidates),
        "{}",
        r.to_markdown()
    );
}

#[test]
fn constraint_units_set_the_output_unit_and_soft_constraints() {
    let r = understand("Choisir le diamètre entre 100 et 600 mm pour minimiser la perte de charge. La perte de charge doit de préférence rester inférieure à 300 kPa.", &manifests());
    let c = &r.spec.constraints[0];
    assert_eq!(
        (c.variable.as_str(), c.unit.as_str(), c.bound, c.hard),
        ("pressure_drop", "kPa", 300.0, false)
    );
    assert_eq!(r.spec.objectives[0].unit, "kPa");
    assert!(r.has(AmbiguityKind::RangeRoleUnclear) == false);
}

// ------------------------------------------------------------------ extraction metrics

/// (sentence, expected quantities): ('v', value, 0, unit) or ('r', low, high, unit); "" = dimensionless.
fn labelled() -> Vec<(&'static str, Vec<(char, f64, f64, &'static str)>)> {
    vec![
        ("La charge vaut 1 kN.", vec![('v', 1.0, 0.0, "kN")]),
        (
            "Le module d'Young est de 210 GPa.",
            vec![('v', 210.0, 0.0, "GPa")],
        ),
        ("La vitesse est de 2,5 m/s.", vec![('v', 2.5, 0.0, "m/s")]),
        (
            "La masse volumique vaut 1000 kg/m³.",
            vec![('v', 1000.0, 0.0, "kg/m^3")],
        ),
        (
            "La viscosité est de 1 mPa·s.",
            vec![('v', 1.0, 0.0, "mPa*s")],
        ),
        ("Une pression de 1,5e3 Pa.", vec![('v', 1500.0, 0.0, "Pa")]),
        (
            "Le module vaut 2,1×10^11 Pa.",
            vec![('v', 2.1e11, 0.0, "Pa")],
        ),
        ("Une masse de 1 000 kg.", vec![('v', 1000.0, 0.0, "kg")]),
        (
            "La température minimale est de −40 °C.",
            vec![('v', -40.0, 0.0, "°C")],
        ),
        (
            "Choisir k entre 0,1 et 1 1/s.",
            vec![('r', 0.1, 1.0, "1/s")],
        ),
        (
            "La surface varie de 300 à 1200 cm².",
            vec![('r', 300.0, 1200.0, "cm^2")],
        ),
        (
            "La surface est comprise dans 300–1200 cm².",
            vec![('r', 300.0, 1200.0, "cm^2")],
        ),
        ("La puissance vaut 14 ± 2 W.", vec![('v', 14.0, 0.0, "W")]),
        ("Le rendement vaut 0,28 ± 0,02.", vec![('v', 0.28, 0.0, "")]),
        ("Un rendement de 25 %.", vec![('v', 25.0, 0.0, "%")]),
        ("La durée est de 4 h.", vec![('v', 4.0, 0.0, "h")]),
        ("Un nanosatellite 3U en orbite.", vec![]),
        ("Le coefficient J2 est connu.", vec![]),
        (
            "La tension est de 12 V et le courant de 2 A.",
            vec![('v', 12.0, 0.0, "V"), ('v', 2.0, 0.0, "A")],
        ),
        (
            "La gravité vaut 9,81 m/s².",
            vec![('v', 9.81, 0.0, "m/s^2")],
        ),
        (
            "La voiture roule à 100 km/h.",
            vec![('v', 100.0, 0.0, "km/h")],
        ),
        (
            "La batterie a une capacité de 2 Ah.",
            vec![('v', 2.0, 0.0, "Ah")],
        ),
        (
            "L'énergie stockée vaut 10 kWh.",
            vec![('v', 10.0, 0.0, "kWh")],
        ),
        (
            "La perte de charge ne doit pas dépasser 3 bar.",
            vec![('v', 3.0, 0.0, "bar")],
        ),
        (
            "Choose the capacity between 2 and 10 Ah.",
            vec![('r', 2.0, 10.0, "Ah")],
        ),
        (
            "The pressure drop must be at most 5 bar.",
            vec![('v', 5.0, 0.0, "bar")],
        ),
        ("The grid frequency is 50 Hz.", vec![('v', 50.0, 0.0, "Hz")]),
        (
            "The second moment is 1e-6 m^4.",
            vec![('v', 1e-6, 0.0, "m^4")],
        ),
        ("The decay rate is 0.5 1/s.", vec![('v', 0.5, 0.0, "1/s")]),
        ("The supply voltage is 220 V.", vec![('v', 220.0, 0.0, "V")]),
        (
            "Le diamètre est compris entre 100 mm et 0,6 m.",
            vec![('r', 0.1, 0.6, "m")],
        ),
        ("Budget : 400 simulations.", vec![('v', 400.0, 0.0, "")]),
        ("La simulation couvre 2 orbites.", vec![('v', 2.0, 0.0, "")]),
        ("En 2025, la longueur vaut 3 m.", vec![('v', 3.0, 0.0, "m")]),
        (
            "Le satellite pèse 4,5 kg pour 30 W consommés.",
            vec![('v', 4.5, 0.0, "kg"), ('v', 30.0, 0.0, "W")],
        ),
        ("La rugosité vaut 0.05 mm.", vec![('v', 0.05, 0.0, "mm")]),
    ]
}

fn matches(q: &ExtractedQuantity, e: &(char, f64, f64, &str)) -> bool {
    let unit = q.unit.clone().unwrap_or_else(|| "1".into());
    let eu = if e.3.is_empty() { "1" } else { e.3 };
    if simorph_units::check_same_dimension(&unit, eu).is_err() {
        return false;
    }
    match (&q.kind, e.0) {
        (QKind::Value { value, .. }, 'v') => close(to_si(*value, &unit), to_si(e.1, eu)),
        (QKind::Range { low, high }, 'r') => {
            close(to_si(*low, &unit), to_si(e.1, eu)) && close(to_si(*high, &unit), to_si(e.2, eu))
        }
        _ => false,
    }
}

#[test]
fn quantity_extraction_precision_and_recall() {
    let set = labelled();
    assert!(set.len() >= 30);
    let (mut tp, mut extracted, mut expected) = (0usize, 0usize, 0usize);
    let mut errors = vec![];
    for (s, exp) in &set {
        let got = extract_quantities(s);
        extracted += got.len();
        expected += exp.len();
        let mut used = vec![false; got.len()];
        for e in exp {
            if let Some(i) = (0..got.len()).find(|&i| !used[i] && matches(&got[i], e)) {
                used[i] = true;
                tp += 1;
            } else {
                errors.push(format!("missed {e:?} in « {s} » (got {got:?})"));
            }
        }
        for (i, u) in used.iter().enumerate() {
            if !u {
                errors.push(format!("spurious {:?} in « {s} »", got[i].text));
            }
        }
    }
    let precision = tp as f64 / extracted.max(1) as f64;
    let recall = tp as f64 / expected.max(1) as f64;
    println!("quantity extraction on {} sentences: precision {precision:.3} ({tp}/{extracted}), recall {recall:.3} ({tp}/{expected})", set.len());
    for e in &errors {
        println!("  {e}");
    }
    assert!(
        precision >= 0.9 && recall >= 0.9,
        "precision {precision:.3} recall {recall:.3}\n{}",
        errors.join("\n")
    );
}
