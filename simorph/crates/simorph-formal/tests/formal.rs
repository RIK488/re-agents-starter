//! End-to-end tests of the formal-verification engine.  Each test prints
//! its measured solver times (`cargo test -p simorph-formal -- --nocapture`).
//! Tests are skipped with a message when the Python helper or Z3 is absent.

use simorph_core::*;
use simorph_formal::*;
use std::collections::BTreeMap;

fn available() -> bool {
    match tool_versions() {
        Ok(v) if v["z3"].is_string() => true,
        other => {
            eprintln!("SKIP: z3 helper unavailable: {other:?}");
            false
        }
    }
}

fn opts() -> ProveOptions {
    ProveOptions::default()
}

fn beam_problem(l_hi: f64) -> Problem {
    Problem::new(
        "cantilever-deflection",
        "δ = P L³/(3 E I) ≤ 0.02 m sur toute la boîte d'entrées",
        Formula::parse_atom("P*L^3/(3*E*I) <= 0.02").unwrap(),
    )
    .real("P", 0.0, 1000.0)
    .real("L", 1.0, l_hi)
    .real("E", 2e11, 2.2e11)
    .real("I", 1e-5, 2e-5)
}

fn run_beam(env: &BTreeMap<String, f64>) -> f64 {
    use simorph_plugin_engineering::beam::Cantilever;
    let c = Cantilever::new();
    let req = TaskRequest::new(c.manifest().id)
        .input("length", env["L"])
        .input("load", env["P"])
        .input("youngs_modulus", env["E"])
        .input("second_moment", env["I"]);
    run_component(&c, &req, &ExecutionControl::default())
        .unwrap()
        .get_f64("tip_deflection")
        .unwrap()
}

#[test]
fn a_beam_constraint_proved_on_box_and_refuted_on_larger_box() {
    if !available() {
        return;
    }
    let cert = prove(&beam_problem(2.0), &opts()).unwrap();
    eprintln!("[a] proof: {}", cert.summary());
    assert_eq!(cert.verdict, Verdict::Proved);
    assert!(cert.rerun_agrees);
    let iv = cert
        .rechecks
        .iter()
        .find(|r| r.method.starts_with("interval"))
        .unwrap();
    assert_eq!(iv.agreement, Agreement::Agrees, "{iv:?}");
    assert!(
        iv.rigorous,
        "only + − × ÷ and integer powers: rigorous enclosure"
    );
    assert!(cert.externally_checked);
    cert.verify_integrity().unwrap();

    let refuted = prove(&beam_problem(10.0), &opts()).unwrap();
    eprintln!("[a] refutation: {}", refuted.summary());
    let cx = match &refuted.verdict {
        Verdict::Refuted {
            counterexample,
            exact,
        } => {
            eprintln!("[a] counterexample (exact) {exact:?}");
            counterexample.clone()
        }
        other => panic!("expected refutation, got {other:?}"),
    };
    assert!(refuted.externally_checked, "{:?}", refuted.rechecks);
    let delta = run_beam(&cx);
    eprintln!("[a] engineering component at counterexample: δ = {delta:.6} m");
    assert!(delta > 0.02, "component confirms the violation");
}

#[test]
fn b_controller_inductive_invariant_and_bounded_unrolling() {
    if !available() {
        return;
    }
    let step = |a: [[f64; 2]; 2]| {
        let x1 = format!("{}*x1 + {}*x2", a[0][0], a[0][1]);
        let x2 = format!("{}*x1 + {}*x2", a[1][0], a[1][1]);
        Formula::and(vec![
            Formula::parse_atom(&format!("{x1} <= 1")).unwrap(),
            Formula::parse_atom(&format!("{x1} >= -1")).unwrap(),
            Formula::parse_atom(&format!("{x2} <= 1")).unwrap(),
            Formula::parse_atom(&format!("{x2} >= -1")).unwrap(),
        ])
    };
    let stable = [[0.5, 0.4], [-0.3, 0.6]];
    let p = Problem::new(
        "box-invariant",
        "la boîte |x|∞ ≤ 1 est invariante pour x⁺ = A x",
        step(stable),
    )
    .real("x1", -1.0, 1.0)
    .real("x2", -1.0, 1.0);
    let c = prove(&p, &opts()).unwrap();
    eprintln!("[b] stable: {}", c.summary());
    assert_eq!(c.verdict, Verdict::Proved);
    assert!(
        c.rechecks
            .iter()
            .any(|r| r.method.starts_with("linear") && r.agreement == Agreement::Agrees),
        "{:?}",
        c.rechecks
    );
    assert!(c.externally_checked);

    let unstable = [[0.9, 0.4], [-0.3, 0.6]];
    let p2 = Problem::new(
        "box-invariant-unstable",
        "même énoncé avec ‖A‖∞ = 1,3",
        step(unstable),
    )
    .real("x1", -1.0, 1.0)
    .real("x2", -1.0, 1.0);
    let c2 = prove(&p2, &opts()).unwrap();
    eprintln!("[b] unstable: {}", c2.summary());
    let cx = match &c2.verdict {
        Verdict::Refuted { counterexample, .. } => counterexample.clone(),
        other => panic!("{other:?}"),
    };
    let nx = (unstable[0][0] * cx["x1"] + unstable[0][1] * cx["x2"])
        .abs()
        .max((unstable[1][0] * cx["x1"] + unstable[1][1] * cx["x2"]).abs());
    assert!(nx > 1.0, "next state leaves the box: {nx}");

    // bounded unrolling: x_k+1 = A x_k for 8 steps from the unit box ⇒ |x_8|∞ ≤ 0.9^8 < 0.44
    let n = 8;
    let mut q = Problem::new(
        "contraction-8-steps",
        "|x₈|∞ ≤ 0,44 pour tout x₀ dans la boîte unité",
        Formula::True,
    )
    .real("x1_0", -1.0, 1.0)
    .real("x2_0", -1.0, 1.0);
    for k in 1..=n {
        q = q
            .free_real(&format!("x1_{k}"))
            .free_real(&format!("x2_{k}"));
        q = q
            .hypothesis(
                Formula::parse_atom(&format!(
                    "x1_{k} = {}*x1_{} + {}*x2_{}",
                    stable[0][0],
                    k - 1,
                    stable[0][1],
                    k - 1
                ))
                .unwrap(),
            )
            .hypothesis(
                Formula::parse_atom(&format!(
                    "x2_{k} = {}*x1_{} + {}*x2_{}",
                    stable[1][0],
                    k - 1,
                    stable[1][1],
                    k - 1
                ))
                .unwrap(),
            );
    }
    q.goal = Formula::and(vec![
        Formula::parse_atom(&format!("x1_{n} <= 0.44")).unwrap(),
        Formula::parse_atom(&format!("x1_{n} >= -0.44")).unwrap(),
        Formula::parse_atom(&format!("x2_{n} <= 0.44")).unwrap(),
        Formula::parse_atom(&format!("x2_{n} >= -0.44")).unwrap(),
    ]);
    let c3 = prove(&q, &opts()).unwrap();
    eprintln!("[b] unrolled: {}", c3.summary());
    assert_eq!(c3.verdict, Verdict::Proved);
    assert!(c3.externally_checked, "{:?}", c3.rechecks);
}

fn beam_spec(max_deflection: f64) -> ProblemSpec {
    let mut p = ProblemSpec::new("Poutre console", "flèche admissible");
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
    p.constraints.push(Constraint {
        name: "flèche".into(),
        variable: "tip_deflection".into(),
        kind: ConstraintKind::UpperBound,
        bound: max_deflection,
        unit: "mm".into(),
        tolerance: 0.0,
        hard: true,
    });
    p
}

#[test]
fn c_problem_spec_feasibility_with_executed_witness() {
    if !available() {
        return;
    }
    let outputs = BTreeMap::from([(
        "tip_deflection".to_string(),
        Expr::parse("load*length^3/(3*youngs_modulus*second_moment)").unwrap(),
    )]);
    let f = spec_feasibility(&beam_spec(20.0), &outputs, &opts()).unwrap();
    eprintln!(
        "[c] feasible: {} ; witness {:?} (unités du problème) / {:?} (SI)",
        f.certificate.summary(),
        f.witness_in_problem_units,
        f.witness_si
    );
    let w = f.witness_si.clone().expect("witness");
    assert!(
        f.certificate.externally_checked,
        "{:?}",
        f.certificate.rechecks
    );
    let env = BTreeMap::from([
        ("L".to_string(), w["length"]),
        ("I".to_string(), w["second_moment"]),
        ("P".to_string(), 1000.0),
        ("E".to_string(), 2.1e11),
    ]);
    let delta = run_beam(&env);
    eprintln!("[c] component at witness: δ = {delta:.6e} m");
    assert!(delta <= 0.02 + 1e-12);
    let pu = f.witness_in_problem_units.unwrap();
    assert!(pu["length"] >= 1000.0 - 1e-9 && pu["length"] <= 3000.0 + 1e-9);

    let inf = spec_feasibility(&beam_spec(0.001), &outputs, &opts()).unwrap();
    eprintln!("[c] infeasible: {}", inf.certificate.summary());
    assert_eq!(inf.certificate.verdict, Verdict::Infeasible);
    assert!(
        inf.certificate
            .rechecks
            .iter()
            .any(|r| r.method.starts_with("interval") && r.agreement == Agreement::Agrees),
        "{:?}",
        inf.certificate.rechecks
    );
    assert!(inf.certificate.externally_checked);
}

#[test]
fn d_scheduler_admission_never_exceeds_capacity() {
    if !available() {
        return;
    }
    let demands = [3.0, 2.0, 4.0, 1.0, 2.0];
    let cap = 6.0;
    let used = |k: usize| -> String {
        if k == 0 {
            "0".into()
        } else {
            (0..k)
                .map(|j| format!("a{j}*{}", demands[j]))
                .collect::<Vec<_>>()
                .join(" + ")
        }
    };
    let build = |correct: bool| {
        let goal = Formula::parse_atom(&format!("{} <= {cap}", used(demands.len()))).unwrap();
        let mut p = Problem::new(
            if correct {
                "admission-greedy"
            } else {
                "admission-buggy"
            },
            "la somme des demandes admises ne dépasse jamais la capacité du pool",
            goal,
        );
        for (i, d) in demands.iter().enumerate() {
            p = p.int(&format!("a{i}"), 0.0, 1.0);
            let guard = if correct {
                format!("{} + {d} <= {cap}", used(i))
            } else {
                format!("{d} <= {cap}")
            };
            p = p.hypothesis(Formula::implies(
                Formula::parse_atom(&format!("a{i} = 1")).unwrap(),
                Formula::parse_atom(&guard).unwrap(),
            ));
        }
        p
    };
    let ok = prove(&build(true), &opts()).unwrap();
    eprintln!("[d] greedy: {}", ok.summary());
    assert_eq!(ok.verdict, Verdict::Proved);
    assert!(ok
        .rechecks
        .iter()
        .any(|r| r.method == "exhaustive-enumeration" && r.agreement == Agreement::Agrees));
    assert!(ok.externally_checked);
    let bad = prove(&build(false), &opts()).unwrap();
    eprintln!("[d] buggy: {}", bad.summary());
    match &bad.verdict {
        Verdict::Refuted { counterexample, .. } => {
            let total: f64 = (0..demands.len())
                .map(|i| counterexample[&format!("a{i}")] * demands[i])
                .sum();
            assert!(total > cap, "{counterexample:?}");
        }
        other => panic!("{other:?}"),
    }
    assert!(bad.externally_checked);
}

#[test]
fn symbolic_ode_equivalence_and_jacobian() {
    if !available() {
        return;
    }
    let ranges = BTreeMap::from([
        ("t".to_string(), (0.0, 3.0)),
        ("k".to_string(), (0.1, 2.0)),
        ("x_0".to_string(), (0.5, 2.0)),
    ]);
    let rhs = BTreeMap::from([("x".to_string(), Expr::parse("-k*x").unwrap())]);
    let good = verify_ode_solution(
        "t",
        &rhs,
        &BTreeMap::from([("x".to_string(), Expr::parse("x_0*exp(-k*t)").unwrap())]),
        &ranges,
    )
    .unwrap();
    eprintln!(
        "[sym] decay: zero={} native={:.2e} sympy {}",
        good.symbolic_zero, good.native_spot_max, good.sympy_version
    );
    assert!(good.verified && good.symbolic_zero);
    let bad = verify_ode_solution(
        "t",
        &rhs,
        &BTreeMap::from([("x".to_string(), Expr::parse("x_0*exp(-2*k*t)").unwrap())]),
        &ranges,
    )
    .unwrap();
    assert!(!bad.verified && !bad.symbolic_zero, "{bad:?}");
    let osc_rhs = BTreeMap::from([
        ("v".to_string(), Expr::parse("-w^2*x").unwrap()),
        ("x".to_string(), Expr::parse("v").unwrap()),
    ]);
    let osc_sol = BTreeMap::from([
        ("v".to_string(), Expr::parse("-A*w*sin(w*t)").unwrap()),
        ("x".to_string(), Expr::parse("A*cos(w*t)").unwrap()),
    ]);
    let r = BTreeMap::from([
        ("t".to_string(), (0.0, 6.0)),
        ("w".to_string(), (0.5, 3.0)),
        ("A".to_string(), (0.1, 2.0)),
    ]);
    assert!(
        verify_ode_solution("t", &osc_rhs, &osc_sol, &r)
            .unwrap()
            .verified
    );

    let units = BTreeMap::from([
        ("a".to_string(), "m".to_string()),
        ("mu".to_string(), "m^3/s^2".to_string()),
    ]);
    let kr = BTreeMap::from([
        ("a".to_string(), (6.5e6, 4.2e7)),
        ("mu".to_string(), (3.9e14, 4.0e14)),
    ]);
    let eq = verify_equivalence(
        &Expr::parse("2*pi*sqrt(a^3/mu)").unwrap(),
        &Expr::parse("2*pi*a*sqrt(a)/sqrt(mu)").unwrap(),
        &kr,
        Some(&units),
    )
    .unwrap();
    eprintln!(
        "[sym] kepler: diff={:?} dim={:?}",
        eq.residual, eq.dimensional
    );
    assert!(eq.verified);
    assert_eq!(eq.dimensional, Some(Ok("[T]".to_string())));
    let neq = verify_equivalence(
        &Expr::parse("2*pi*sqrt(a^3/mu)").unwrap(),
        &Expr::parse("2*pi*sqrt(a^2/mu)").unwrap(),
        &kr,
        Some(&units),
    )
    .unwrap();
    assert!(!neq.verified);
    assert!(matches!(neq.dimensional, Some(Err(_))));

    let vdp = vec![
        Expr::parse("y").unwrap(),
        Expr::parse("mu*(1 - x^2)*y - x").unwrap(),
    ];
    let j = jacobian(
        &["x".into(), "y".into()],
        &vdp,
        &BTreeMap::from([("x".into(), 0.7), ("y".into(), -1.3), ("mu".into(), 5.0)]),
    )
    .unwrap();
    eprintln!(
        "[sym] vdp jacobian {:?} max rel diff vs FD {:.2e}",
        j.symbolic, j.max_relative_difference
    );
    assert!(j.agrees);
}

#[test]
fn evidence_integration_only_checked_proofs_become_proven() {
    if !available() {
        return;
    }
    // proved + externally checked → MathematicallyProven
    let claim =
        Claim::hypothesis("la flèche de la console reste ≤ 20 mm sur la boîte de conception");
    let (rec, claim, cert) = prove_claim(claim, &beam_problem(2.0), &opts()).unwrap();
    assert!(cert.externally_checked);
    assert_eq!(
        rec.verdict,
        EvidenceLevel::MathematicallyProven,
        "{:?}",
        rec.rationale
    );
    assert_eq!(claim.level, EvidenceLevel::MathematicallyProven);
    assert_eq!(claim.validations.len(), 1);

    // the same proof cannot lift a Simulated claim (transition not allowed)
    let mut sim = Claim::hypothesis("x");
    sim.promote(EvidenceLevel::Simulated, ValidationId::new())
        .unwrap();
    let (rec_s, sim, cert_s) = prove_claim(sim, &beam_problem(2.0), &opts()).unwrap();
    assert_eq!(sim.level, EvidenceLevel::Simulated);
    assert!(cert_s.externally_checked, "{}", cert_s.summary());
    assert_eq!(
        rec_s.verdict,
        EvidenceLevel::Simulated,
        "validator refuses Simulated → Proven"
    );
    assert!(
        sim.caveats.iter().any(|c| c.contains("transition refusée")),
        "{:?}",
        sim.caveats
    );

    // refuted → never proven
    let (rec2, c2, _) =
        prove_claim(Claim::hypothesis("faux"), &beam_problem(10.0), &opts()).unwrap();
    assert_ne!(rec2.verdict, EvidenceLevel::MathematicallyProven);
    assert_eq!(c2.level, EvidenceLevel::UnverifiedHypothesis);

    // timeout/unknown → never proven (Fermat-like cubic with 1 ms budget)
    let hard = Problem::new(
        "cubes",
        "x³ + y³ ≠ z³",
        Formula::not(Formula::parse_atom("x^3 + y^3 = z^3").unwrap()),
    )
    .int("x", 1.0, 1e6)
    .int("y", 1.0, 1e6)
    .int("z", 1.0, 1e6);
    let quick = ProveOptions {
        timeout_ms: 1,
        ..ProveOptions::default()
    };
    let (rec3, c3, cert3) = prove_claim(Claim::hypothesis("cubes"), &hard, &quick).unwrap();
    eprintln!("[ev] hard problem with 1 ms: {:?}", cert3.verdict);
    assert!(!cert3.externally_checked);
    assert_ne!(rec3.verdict, EvidenceLevel::MathematicallyProven);
    assert_eq!(c3.level, EvidenceLevel::UnverifiedHypothesis);

    // tampering with the stored SMT-LIB text is detected
    let mut forged = cert.clone();
    forged.smtlib = forged.smtlib.replace("0.02", "0.2");
    assert!(matches!(
        forged.verify_integrity(),
        Err(SimorphError::Protected(_))
    ));
    assert!(matches!(
        recheck(&forged, &opts()),
        Err(SimorphError::Protected(_))
    ));
    let mut c4 = Claim::hypothesis("forged");
    let (rec4, c4) = judge_claim(&mut c4, &forged).unwrap();
    assert_ne!(rec4.verdict, EvidenceLevel::MathematicallyProven);
    assert_eq!(c4.level, EvidenceLevel::UnverifiedHypothesis);
    // re-hashing the forged text is not enough: it no longer matches the stored problem
    forged.smtlib_sha256 = simorph_core::hashing::sha256_hex(forged.smtlib.as_bytes());
    assert!(forged.verify_integrity().is_err());
    // honest recheck of the original succeeds
    let again = recheck(&cert, &opts()).unwrap();
    assert!(again.externally_checked);
}

#[test]
fn lean_backend_reports_absence_precisely() {
    match LeanBackend::detect() {
        Err(SimorphError::NotImplemented(msg)) => {
            eprintln!("[lean] {msg}");
            assert!(msg.contains("`lean` not found on PATH"));
        }
        Ok(lean) => {
            let r = lean
                .check("theorem two_plus_two : 2 + 2 = 4 := rfl\n")
                .unwrap();
            assert!(r.accepted, "{}", r.output);
            let bad = lean.check("theorem bad : 2 + 2 = 5 := rfl\n").unwrap();
            assert!(!bad.accepted);
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn transcendental_goals_are_refused_by_smt_not_faked() {
    let p = Problem::new(
        "kepler",
        "T > 0",
        Formula::parse_atom("2*pi*sqrt(a^3/mu) > 0").unwrap(),
    )
    .real("a", 1.0, 2.0)
    .real("mu", 1.0, 2.0);
    assert!(matches!(
        prove(&p, &opts()),
        Err(SimorphError::NotImplemented(_))
    ));
}
