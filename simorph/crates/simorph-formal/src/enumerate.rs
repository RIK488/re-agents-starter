//! Exhaustive enumeration for finite (Int/Bool, bounded) problems: an
//! independent, trivially-correct decision procedure used as a second
//! opinion when the domain is small.

use crate::formula::{Problem, Sort};
use simorph_core::Result;
use std::collections::BTreeMap;

/// Maximum number of assignments enumerated.
pub const MAX_ASSIGNMENTS: u64 = 2_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Enumeration {
    /// Goal holds for every assignment satisfying the premises.
    Holds {
        assignments: u64,
        premises_satisfied: u64,
    },
    Violated {
        assignment: BTreeMap<String, f64>,
    },
    /// At least one assignment satisfies premises ∧ goal (feasibility).
    Witness {
        assignment: BTreeMap<String, f64>,
    },
    /// No assignment satisfies premises ∧ goal.
    NoWitness {
        assignments: u64,
    },
    NotApplicable(String),
}

fn domains(p: &Problem) -> std::result::Result<Vec<(String, i64, i64)>, String> {
    let mut d = vec![];
    let mut total: u64 = 1;
    for (n, s) in &p.variables {
        let (lo, hi) = match s {
            Sort::Bool => (0, 1),
            Sort::Int => match p.bounds.get(n) {
                Some((l, h)) => (l.ceil() as i64, h.floor() as i64),
                None => return Err(format!("Int `{n}` is unbounded")),
            },
            Sort::Real => return Err(format!("`{n}` is Real")),
        };
        total = total.saturating_mul((hi - lo + 1).max(0) as u64);
        if total > MAX_ASSIGNMENTS {
            return Err(format!("domain larger than {MAX_ASSIGNMENTS} assignments"));
        }
        d.push((n.clone(), lo, hi));
    }
    Ok(d)
}

fn for_each(
    p: &Problem,
    mut f: impl FnMut(&BTreeMap<String, f64>) -> Result<bool>,
) -> std::result::Result<Result<u64>, String> {
    let d = domains(p)?;
    let mut idx: Vec<i64> = d.iter().map(|(_, l, _)| *l).collect();
    let mut count = 0u64;
    loop {
        let env: BTreeMap<String, f64> = d
            .iter()
            .zip(&idx)
            .map(|((n, _, _), v)| (n.clone(), *v as f64))
            .collect();
        count += 1;
        match f(&env) {
            Ok(true) => {}
            Ok(false) => return Ok(Ok(count)),
            Err(e) => return Ok(Err(e)),
        }
        // increment mixed-radix counter
        let mut k = 0;
        loop {
            if k == d.len() {
                return Ok(Ok(count));
            }
            idx[k] += 1;
            if idx[k] > d[k].2 {
                idx[k] = d[k].1;
                k += 1;
            } else {
                break;
            }
        }
    }
}

/// Decide `premises ⇒ goal` by enumeration.
pub fn decide_proof(p: &Problem) -> Result<Enumeration> {
    let mut violated = None;
    let mut sat = 0u64;
    let r = for_each(p, |env| {
        if p.premises_hold(env)? {
            sat += 1;
            if !p.goal.eval(env)? {
                violated = Some(env.clone());
                return Ok(false);
            }
        }
        Ok(true)
    });
    match r {
        Err(why) => Ok(Enumeration::NotApplicable(why)),
        Ok(Err(e)) => Err(e),
        Ok(Ok(n)) => Ok(match violated {
            Some(a) => Enumeration::Violated { assignment: a },
            None => Enumeration::Holds {
                assignments: n,
                premises_satisfied: sat,
            },
        }),
    }
}

/// Decide feasibility of `premises ∧ goal` by enumeration.
pub fn decide_feasibility(p: &Problem) -> Result<Enumeration> {
    let mut witness = None;
    let r = for_each(p, |env| {
        if p.premises_hold(env)? && p.goal.eval(env)? {
            witness = Some(env.clone());
            return Ok(false);
        }
        Ok(true)
    });
    match r {
        Err(why) => Ok(Enumeration::NotApplicable(why)),
        Ok(Err(e)) => Err(e),
        Ok(Ok(n)) => Ok(match witness {
            Some(a) => Enumeration::Witness { assignment: a },
            None => Enumeration::NoWitness { assignments: n },
        }),
    }
}
