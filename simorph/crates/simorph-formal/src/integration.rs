//! Integration with SIMORPH's evidence model.
//!
//! Only an SMT certificate that is decisive, re-run identically in a fresh
//! process and confirmed by an independent method (`externally_checked`)
//! can lead the validator to grant `MathematicallyProven` — and then only
//! for the bounded statement encoded in the certificate.

use crate::expr::Expr;
use crate::formula::{Cmp, Formula, Problem, Sort};
use crate::prover::{check_feasibility, prove, Certificate, ProveOptions, Verdict};
use simorph_core::{
    Claim, ComponentId, ConstraintKind, EvidenceLevel, ProblemSpec, Result, SimorphError,
    TaskRequest, TaskResult,
};
use simorph_validation::{
    classify, CheckResult, ProofArtifact, Prover, ValidationRecord, ValidationTarget,
};
use std::collections::BTreeMap;

/// Proof artifact for the validator.  `certificate_hash` is the content
/// hash of the whole certificate (script, verdict, re-checks).
pub fn to_proof_artifact(cert: &Certificate) -> ProofArtifact {
    ProofArtifact {
        prover: Prover::Z3,
        statement: format!("{} — {}", cert.problem.name, cert.problem.statement),
        certificate_hash: cert.content_hash(),
        externally_checked: cert.externally_checked
            && matches!(cert.verdict, Verdict::Proved)
            && cert.verify_integrity().is_ok(),
    }
}

/// Checks derived from a certificate (all must pass for a proof).
pub fn certificate_checks(cert: &Certificate) -> Vec<CheckResult> {
    let mut v = vec![];
    v.push(match cert.verify_integrity() {
        Ok(()) => CheckResult::pass(
            "formal:integrity",
            format!("SHA-256 {}", &cert.smtlib_sha256[..16]),
        ),
        Err(e) => CheckResult::fail("formal:integrity", e.to_string()),
    });
    v.push(match &cert.verdict {
        Verdict::Proved => CheckResult::pass(
            "formal:smt",
            format!(
                "{} {} : négation insatisfaisable ({:.1} ms)",
                cert.solver, cert.solver_version, cert.solver_ms
            ),
        ),
        Verdict::Refuted { counterexample, .. } => CheckResult::fail(
            "formal:smt",
            format!("réfuté, contre-exemple {counterexample:?}"),
        ),
        other => CheckResult::fail("formal:smt", format!("non décidé : {other:?}")),
    });
    v.push(if cert.rerun_agrees {
        CheckResult::pass("formal:rerun", "relance du script stocké concordante")
    } else {
        CheckResult::fail("formal:rerun", "relance du script stocké NON concordante")
    });
    for r in &cert.rechecks {
        let name = format!("formal:independent:{}", r.method);
        v.push(match r.agreement {
            crate::prover::Agreement::Disagrees => {
                CheckResult::fail(&name, format!("DÉSACCORD : {}", r.details))
            }
            a => CheckResult::pass(&name, format!("{a:?} : {}", r.details))
                .metric("rigorous", if r.rigorous { 1.0 } else { 0.0 }),
        });
    }
    v
}

/// Try to prove a claim.  Returns the validation record and the claim with
/// its new level: `MathematicallyProven` only when the validator granted it
/// (externally checked certificate) and the transition is allowed from the
/// claim's current level; otherwise the claim keeps its level and receives
/// a caveat explaining why.
pub fn prove_claim(
    mut claim: Claim,
    problem: &Problem,
    opts: &ProveOptions,
) -> Result<(ValidationRecord, Claim, Certificate)> {
    let cert = prove(problem, opts)?;
    let (record, claim2) = judge_claim(&mut claim, &cert)?;
    Ok((record, claim2, cert))
}

/// Apply an existing certificate to a claim (used for re-validation of
/// stored certificates; a tampered certificate fails integrity).
pub fn judge_claim(claim: &mut Claim, cert: &Certificate) -> Result<(ValidationRecord, Claim)> {
    let checks = certificate_checks(cert);
    let artifact = to_proof_artifact(cert);
    let pseudo = TaskResult::new(
        &TaskRequest::new(ComponentId::deterministic("simorph-formal")),
        claim.level,
    );
    let record = classify(
        ValidationTarget::Claim {
            statement: claim.statement.clone(),
        },
        &pseudo,
        checks,
        &[],
        Some(&artifact),
        None,
    );
    claim.caveats.push(format!(
        "certificat formel {} : {}",
        &artifact.certificate_hash[..16],
        cert.summary()
    ));
    if record.verdict == EvidenceLevel::MathematicallyProven
        && claim.level != EvidenceLevel::MathematicallyProven
    {
        if let Err(e) = claim.promote(EvidenceLevel::MathematicallyProven, record.id) {
            claim.caveats.push(format!(
                "démonstration valide mais transition refusée depuis « {} » : {e}",
                claim.level.label_fr()
            ));
        }
    } else if record.verdict != EvidenceLevel::MathematicallyProven && artifact.externally_checked {
        // valid, checked proof, but the validator refused the transition
        // (e.g. from `Simulated`, which a proof cannot turn into a theorem)
        claim.caveats.push(format!("démonstration valide mais transition refusée depuis « {} » : le validateur conserve le niveau", claim.level.label_fr()));
    } else if let Verdict::Refuted { counterexample, .. } = &cert.verdict {
        claim.caveats.push(format!(
            "énoncé réfuté par le solveur ; contre-exemple : {counterexample:?}"
        ));
    }
    Ok((record, claim.clone()))
}

/// Result of a constraint-feasibility analysis of a [`ProblemSpec`].
#[derive(Debug, Clone)]
pub struct SpecFeasibility {
    pub problem: Problem,
    pub certificate: Certificate,
    /// Witness in the *problem's* design-variable units.
    pub witness_in_problem_units: Option<BTreeMap<String, f64>>,
    /// Witness in SI units (what the expressions use).
    pub witness_si: Option<BTreeMap<String, f64>>,
}

/// Are the hard constraints of `spec` jointly satisfiable inside the
/// design-variable bounds?  `outputs` gives each constrained output as an
/// expression of design variables and knowns, **in SI units**.  Knowns and
/// bounds are converted to SI with simorph-units.
pub fn spec_feasibility(
    spec: &ProblemSpec,
    outputs: &BTreeMap<String, Expr>,
    opts: &ProveOptions,
) -> Result<SpecFeasibility> {
    let si = |v: f64, u: &str| -> Result<f64> { Ok(simorph_units::parse_unit(u)?.to_si(v)) };
    let knowns: BTreeMap<String, Expr> = spec
        .knowns
        .iter()
        .map(|q| Ok((q.name.clone(), Expr::num(si(q.value, &q.unit)?))))
        .collect::<Result<_>>()?;
    let mut atoms = vec![];
    for c in spec.constraints.iter().filter(|c| c.hard) {
        let e = outputs
            .get(&c.variable)
            .ok_or_else(|| {
                SimorphError::InvalidInput(format!(
                    "no expression for constrained output `{}`",
                    c.variable
                ))
            })?
            .substitute(&knowns);
        let b = si(c.bound, &c.unit)?;
        let tol = si(c.tolerance, &c.unit)?.abs();
        match c.kind {
            ConstraintKind::UpperBound => atoms.push(Formula::atom(e, Cmp::Le, Expr::num(b + tol))),
            ConstraintKind::LowerBound => atoms.push(Formula::atom(e, Cmp::Ge, Expr::num(b - tol))),
            ConstraintKind::Equality => {
                atoms.push(Formula::atom(e.clone(), Cmp::Le, Expr::num(b + tol)));
                atoms.push(Formula::atom(e, Cmp::Ge, Expr::num(b - tol)));
            }
        }
    }
    let mut p = Problem::new(&format!("faisabilité : {}", spec.title), "les contraintes dures sont simultanément satisfaisables dans les bornes des variables de conception", Formula::and(atoms));
    for d in &spec.design_variables {
        let (lo, hi) = (si(d.low, &d.unit)?, si(d.high, &d.unit)?);
        p.variables.push((
            d.name.clone(),
            if d.integer { Sort::Int } else { Sort::Real },
        ));
        p.bounds.insert(d.name.clone(), (lo.min(hi), lo.max(hi)));
    }
    let cert = check_feasibility(&p, opts)?;
    let (wsi, wpu) = match &cert.verdict {
        Verdict::Feasible { witness, .. } => {
            let mut pu = BTreeMap::new();
            for d in &spec.design_variables {
                if let Some(v) = witness.get(&d.name) {
                    pu.insert(
                        d.name.clone(),
                        simorph_units::parse_unit(&d.unit)?.from_si(*v),
                    );
                }
            }
            (Some(witness.clone()), Some(pu))
        }
        _ => (None, None),
    };
    Ok(SpecFeasibility {
        problem: p,
        certificate: cert,
        witness_in_problem_units: wpu,
        witness_si: wsi,
    })
}
