//! Formal verification action (subsystem 13): bounded statements proved or
//! refuted by Z3 with independently re-checked certificates.

use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_formal as f;
use simorph_provenance::ProvenanceKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormalVariable {
    pub name: String,
    pub low: f64,
    pub high: f64,
    #[serde(default)]
    pub integer: bool,
}

/// `goal` and `hypotheses` are atoms such as `P*L^3/(3*E*I) <= 0.02`;
/// several goals are combined with AND.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormalRequest {
    pub name: String,
    #[serde(default)]
    pub statement: String,
    pub variables: Vec<FormalVariable>,
    #[serde(default)]
    pub hypotheses: Vec<String>,
    pub goals: Vec<String>,
    /// `prove` (goal holds for every admissible assignment) or `feasibility`.
    #[serde(default = "prove_mode")]
    pub mode: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn prove_mode() -> String {
    "prove".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormalOutput {
    pub certificate: f::Certificate,
    pub evidence: EvidenceLevel,
    pub summary: String,
}

pub fn formal_check(ws: Option<&Workspace>, req: &FormalRequest) -> Result<FormalOutput> {
    let goals: Vec<f::Formula> = req
        .goals
        .iter()
        .map(|g| f::Formula::parse_atom(g))
        .collect::<Result<_>>()?;
    if goals.is_empty() {
        return Err(SimorphError::InvalidInput(
            "at least one goal is required".into(),
        ));
    }
    let goal = if goals.len() == 1 {
        goals.into_iter().next().unwrap()
    } else {
        f::Formula::and(goals)
    };
    let mut p = f::Problem::new(
        &req.name,
        if req.statement.is_empty() {
            &req.name
        } else {
            &req.statement
        },
        goal,
    );
    for v in &req.variables {
        p = if v.integer {
            p.int(&v.name, v.low, v.high)
        } else {
            p.real(&v.name, v.low, v.high)
        };
    }
    for h in &req.hypotheses {
        p = p.hypothesis(f::Formula::parse_atom(h)?);
    }
    let mut opts = f::ProveOptions::default();
    if let Some(t) = req.timeout_ms {
        opts.timeout_ms = t;
    }
    let cert = match req.mode.as_str() {
        "prove" => f::prove(&p, &opts)?,
        "feasibility" => f::check_feasibility(&p, &opts)?,
        other => {
            return Err(SimorphError::InvalidInput(format!(
                "unknown mode `{other}` (prove | feasibility)"
            )))
        }
    };
    let evidence = if cert.externally_checked
        && req.mode == "prove"
        && matches!(cert.verdict, f::Verdict::Proved)
    {
        EvidenceLevel::MathematicallyProven
    } else {
        EvidenceLevel::UnverifiedHypothesis
    };
    let summary = format!(
        "{} — verdict {:?} ({} {}, {:.1} ms) ; relance identique : {} ; contrôle indépendant concordant : {} ; désaccord : {}",
        req.name, cert.verdict, cert.solver, cert.solver_version, cert.solver_ms, cert.rerun_agrees, cert.externally_checked, cert.disagreement
    );
    if let Some(ws) = ws {
        ws.ledger.record(
            ProvenanceKind::Validation,
            format!("vérification formelle : {}", req.name),
            &cert,
            &[],
        )?;
    }
    Ok(FormalOutput {
        certificate: cert,
        evidence,
        summary,
    })
}
