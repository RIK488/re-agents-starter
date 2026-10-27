//! Symbolic verification with SymPy, cross-checked natively.
//!
//! A computer-algebra simplification to zero is strong evidence but SymPy is
//! not a proof kernel: these certificates are *never* turned into
//! `MathematicallyProven` (see [`crate::integration`]); they support
//! `Computed` claims and dimensional/equivalence checks.

use crate::expr::Expr;
use crate::runner::{run_tool, solver_policy};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::hashing::hash_json;
use simorph_core::{Result, SimorphError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymbolicCertificate {
    pub operation: String,
    /// Exact texts sent to SymPy (generated from the validated AST).
    pub inputs: BTreeMap<String, String>,
    pub sympy_version: String,
    /// Simplified residual / difference returned by SymPy.
    pub residual: Vec<String>,
    pub symbolic_zero: bool,
    /// Max |residual| at random points, computed by SymPy (lambdify).
    pub sympy_spot_max: f64,
    /// Max residual computed natively in Rust (independent evaluation).
    pub native_spot_max: f64,
    pub native_tolerance: f64,
    /// Dimensional check result (if units were supplied).
    pub dimensional: Option<std::result::Result<String, String>>,
    pub verified: bool,
    pub sha256: String,
}

fn seal(mut c: SymbolicCertificate) -> SymbolicCertificate {
    c.sha256 = String::new();
    c.sha256 = hash_json(&c).expect("serialisable");
    c
}

fn symbols_of(exprs: &[&Expr], extra: &[&str]) -> Vec<String> {
    let mut s: BTreeSet<String> = exprs.iter().flat_map(|e| e.variables()).collect();
    s.extend(extra.iter().map(|x| x.to_string()));
    s.into_iter().collect()
}

/// Deterministic pseudo-random points (no RNG dependency).
fn points(
    names: &[String],
    ranges: &BTreeMap<String, (f64, f64)>,
    n: usize,
) -> Vec<BTreeMap<String, f64>> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            names
                .iter()
                .map(|k| {
                    let (lo, hi) = ranges.get(k).copied().unwrap_or((0.5, 2.0));
                    (k.clone(), lo + (hi - lo) * next())
                })
                .collect()
        })
        .collect()
}

/// Verify that `solution[s](t, params)` solves `ds/dt = rhs[s]` for every
/// state `s`.  Native check: central finite difference of the solution vs
/// the right-hand side at random points.
pub fn verify_ode_solution(
    time: &str,
    rhs: &BTreeMap<String, Expr>,
    solution: &BTreeMap<String, Expr>,
    ranges: &BTreeMap<String, (f64, f64)>,
) -> Result<SymbolicCertificate> {
    let states: Vec<String> = rhs.keys().cloned().collect();
    if solution.keys().cloned().collect::<Vec<_>>() != states {
        return Err(SimorphError::InvalidInput(
            "solution must give every state".into(),
        ));
    }
    let all: Vec<&Expr> = rhs.values().chain(solution.values()).collect();
    let mut syms = symbols_of(&all, &[time]);
    syms.retain(|s| !states.contains(s));
    let mut sym_all = syms.clone();
    sym_all.extend(states.iter().cloned());
    sym_all.sort();
    sym_all.dedup();
    let req = serde_json::json!({
        "op": "sympy_ode",
        "symbols": sym_all,
        "time": time,
        "states": states,
        "rhs": rhs.iter().map(|(k, v)| (k.clone(), v.to_sympy())).collect::<BTreeMap<_, _>>(),
        "solution": solution.iter().map(|(k, v)| (k.clone(), v.to_sympy())).collect::<BTreeMap<_, _>>(),
        "ranges": ranges,
        "samples": 40,
    });
    let (v, _) = run_tool(&req, &solver_policy())?;
    // native check
    let pts = points(&syms, ranges, 40);
    let mut native = 0.0f64;
    let mut scale = 0.0f64;
    for p in &pts {
        let t = p[time];
        let h = 1e-5 * (1.0 + t.abs());
        let mut env = p.clone();
        for s in &states {
            env.insert(s.clone(), solution[s].eval(p)?);
        }
        for s in &states {
            let mut pp = p.clone();
            pp.insert(time.into(), t + h);
            let fp = solution[s].eval(&pp)?;
            pp.insert(time.into(), t - h);
            let fm = solution[s].eval(&pp)?;
            let d = (fp - fm) / (2.0 * h);
            let r = rhs[s].eval(&env)?;
            native = native.max((d - r).abs());
            scale = scale.max(r.abs()).max(d.abs());
        }
    }
    let tol = 1e-6 * (1.0 + scale);
    let zero = v["is_zero"].as_bool().unwrap_or(false);
    Ok(seal(SymbolicCertificate {
        operation: "ode_solution".into(),
        inputs: [
            ("rhs", req["rhs"].to_string()),
            ("solution", req["solution"].to_string()),
            ("time", time.to_string()),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b))
        .collect(),
        sympy_version: v["sympy"].as_str().unwrap_or("?").into(),
        residual: v["residuals"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        symbolic_zero: zero,
        sympy_spot_max: v["spot_max_abs"].as_f64().unwrap_or(f64::NAN),
        native_spot_max: native,
        native_tolerance: tol,
        dimensional: None,
        verified: zero && native <= tol,
        sha256: String::new(),
    }))
}

/// Verify `a ≡ b` (symbols assumed positive, as physical magnitudes).
/// When `units` is given, both sides must also have the same dimension.
pub fn verify_equivalence(
    a: &Expr,
    b: &Expr,
    ranges: &BTreeMap<String, (f64, f64)>,
    units: Option<&BTreeMap<String, String>>,
) -> Result<SymbolicCertificate> {
    let syms = symbols_of(&[a, b], &[]);
    let req = serde_json::json!({"op": "sympy_equiv", "symbols": syms, "a": a.to_sympy(), "b": b.to_sympy(), "positive": true, "ranges": ranges, "samples": 50});
    let (v, _) = run_tool(&req, &solver_policy())?;
    let mut native = 0.0f64;
    let mut scale = 0.0f64;
    for p in points(&syms, ranges, 50) {
        let (x, y) = (a.eval(&p)?, b.eval(&p)?);
        native = native.max((x - y).abs());
        scale = scale.max(x.abs()).max(y.abs());
    }
    let tol = 1e-9 * (1.0 + scale);
    let dimensional = units.map(|u| match (a.dimension(u), b.dimension(u)) {
        (Ok(da), Ok(db)) if da == db => Ok(format!("[{da}]")),
        (Ok(da), Ok(db)) => Err(format!("[{da}] ≠ [{db}]")),
        (Err(e), _) | (_, Err(e)) => Err(e.to_string()),
    });
    let zero = v["is_zero"].as_bool().unwrap_or(false);
    let dim_ok = dimensional.as_ref().map(|d| d.is_ok()).unwrap_or(true);
    Ok(seal(SymbolicCertificate {
        operation: "equivalence".into(),
        inputs: [
            ("a".to_string(), a.to_sympy()),
            ("b".to_string(), b.to_sympy()),
        ]
        .into_iter()
        .collect(),
        sympy_version: v["sympy"].as_str().unwrap_or("?").into(),
        residual: vec![v["difference"].as_str().unwrap_or("?").to_string()],
        symbolic_zero: zero,
        sympy_spot_max: v["spot_max_abs"].as_f64().unwrap_or(f64::NAN),
        native_spot_max: native,
        native_tolerance: tol,
        dimensional,
        verified: zero && native <= tol && dim_ok,
        sha256: String::new(),
    }))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JacobianCheck {
    pub symbolic: Vec<Vec<String>>,
    pub symbolic_values: Vec<Vec<f64>>,
    pub finite_difference: Vec<Vec<f64>>,
    pub max_relative_difference: f64,
    pub agrees: bool,
    pub sympy_version: String,
}

/// Symbolic Jacobian of `rhs` w.r.t. `states` at `point`, cross-checked
/// against central finite differences of the native evaluator.
pub fn jacobian(
    states: &[String],
    rhs: &[Expr],
    point: &BTreeMap<String, f64>,
) -> Result<JacobianCheck> {
    let refs: Vec<&Expr> = rhs.iter().collect();
    let syms = symbols_of(
        &refs,
        &states.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    );
    let req = serde_json::json!({"op": "sympy_jacobian", "symbols": syms, "states": states, "rhs": rhs.iter().map(|e| e.to_sympy()).collect::<Vec<_>>(), "point": point});
    let (v, _) = run_tool(&req, &solver_policy())?;
    let sym_vals: Vec<Vec<f64>> = serde_json::from_value(v["values"].clone())?;
    let sym_text: Vec<Vec<String>> = serde_json::from_value(v["jacobian"].clone())?;
    let mut fd = vec![vec![0.0; states.len()]; rhs.len()];
    let mut worst = 0.0f64;
    for (i, f) in rhs.iter().enumerate() {
        for (k, s) in states.iter().enumerate() {
            let x = point[s];
            let h = 1e-6 * (1.0 + x.abs());
            let mut p = point.clone();
            p.insert(s.clone(), x + h);
            let a = f.eval(&p)?;
            p.insert(s.clone(), x - h);
            let b = f.eval(&p)?;
            fd[i][k] = (a - b) / (2.0 * h);
            let rel = (fd[i][k] - sym_vals[i][k]).abs() / (1.0 + sym_vals[i][k].abs());
            worst = worst.max(rel);
        }
    }
    Ok(JacobianCheck {
        symbolic: sym_text,
        symbolic_values: sym_vals,
        finite_difference: fd,
        max_relative_difference: worst,
        agrees: worst < 1e-5,
        sympy_version: v["sympy"].as_str().unwrap_or("?").into(),
    })
}
