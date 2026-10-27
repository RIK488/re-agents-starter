//! Second-opinion check for linear real arithmetic with SciPy's HiGHS LP
//! solver (a different algorithm and code base than Z3).

use crate::expr::Expr;
use crate::formula::{Cmp, Problem, Sort};
use crate::runner::run_tool;
use simorph_core::{Result, SimorphError};
use simorph_security::SandboxPolicy;
use std::collections::BTreeMap;

/// Linear system extracted from a problem.
pub struct LinearSystem {
    pub vars: Vec<String>,
    pub a_ub: Vec<Vec<f64>>,
    pub b_ub: Vec<f64>,
    pub a_eq: Vec<Vec<f64>>,
    pub b_eq: Vec<f64>,
    pub bounds: Vec<(Option<f64>, Option<f64>)>,
}

fn row(vars: &[String], m: &BTreeMap<String, f64>) -> Vec<f64> {
    vars.iter()
        .map(|v| m.get(v).copied().unwrap_or(0.0))
        .collect()
}

/// `lhs - rhs` as a linear form.
pub fn linear_difference(lhs: &Expr, rhs: &Expr) -> Option<(BTreeMap<String, f64>, f64)> {
    Expr::sub(lhs.clone(), rhs.clone()).linear_form()
}

/// Build the LP of the premises (bounds + hypotheses) if they are a
/// conjunction of linear atoms over Real/Int variables (Int relaxed to
/// Real: sound for proving, since the relaxation is a superset).
pub fn premises_system(p: &Problem) -> Option<LinearSystem> {
    if p.variables.iter().any(|(_, s)| *s == Sort::Bool) {
        return None;
    }
    let vars: Vec<String> = p.variables.iter().map(|(n, _)| n.clone()).collect();
    let mut sys = LinearSystem {
        vars: vars.clone(),
        a_ub: vec![],
        b_ub: vec![],
        a_eq: vec![],
        b_eq: vec![],
        bounds: vars
            .iter()
            .map(|v| {
                p.bounds
                    .get(v)
                    .map(|(l, h)| (Some(*l), Some(*h)))
                    .unwrap_or((None, None))
            })
            .collect(),
    };
    for h in &p.hypotheses {
        for (l, c, r) in h.conjunct_atoms()? {
            let (m, k) = linear_difference(&l, &r)?;
            // m·x + k  cmp 0
            match c {
                Cmp::Le | Cmp::Lt => {
                    sys.a_ub.push(row(&vars, &m));
                    sys.b_ub.push(-k);
                }
                Cmp::Ge | Cmp::Gt => {
                    sys.a_ub
                        .push(row(&vars, &m).into_iter().map(|x| -x).collect());
                    sys.b_ub.push(k);
                }
                Cmp::Eq => {
                    sys.a_eq.push(row(&vars, &m));
                    sys.b_eq.push(-k);
                }
            }
        }
    }
    Some(sys)
}

/// Optimum of `m·x + k` over the premises (`max == true` → maximise).
/// Returns `None` when the LP is infeasible.
pub fn optimise(
    sys: &LinearSystem,
    m: &BTreeMap<String, f64>,
    k: f64,
    max: bool,
    policy: &SandboxPolicy,
) -> Result<Option<f64>> {
    let req = serde_json::json!({
        "op": "linprog",
        "c": row(&sys.vars, m),
        "sense": if max { "max" } else { "min" },
        "A_ub": sys.a_ub, "b_ub": sys.b_ub, "A_eq": sys.a_eq, "b_eq": sys.b_eq,
        "bounds": sys.bounds,
    });
    let (v, _) = run_tool(&req, policy)?;
    match v["status"].as_i64() {
        Some(0) => Ok(Some(v["value"].as_f64().unwrap_or(f64::NAN) + k)),
        Some(2) => Ok(None),
        Some(3) => Ok(Some(if max {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        })),
        other => Err(SimorphError::Numerical(format!(
            "linprog status {other:?}: {}",
            v["message"]
        ))),
    }
}
