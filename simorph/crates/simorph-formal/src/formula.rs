//! Logical formulas and verification problems.

use crate::expr::{smt_number, smt_symbol, Expr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    Real,
    Int,
    Bool,
}

impl Sort {
    fn smt(self) -> &'static str {
        match self {
            Sort::Real => "Real",
            Sort::Int => "Int",
            Sort::Bool => "Bool",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl Cmp {
    fn smt(self) -> &'static str {
        match self {
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Eq => "=",
            Cmp::Ge => ">=",
            Cmp::Gt => ">",
        }
    }
    pub fn holds(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Eq => a == b,
            Cmp::Ge => a >= b,
            Cmp::Gt => a > b,
        }
    }
}

/// Quantifier-free formula (universal quantification over the declared
/// variables is implicit in [`Problem`]: a goal is proved for *all* values
/// satisfying the bounds and hypotheses).  `BoolVar` refers to a declared
/// Bool constant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Formula {
    True,
    False,
    BoolVar { name: String },
    Atom { lhs: Expr, cmp: Cmp, rhs: Expr },
    Not { f: Box<Formula> },
    And { fs: Vec<Formula> },
    Or { fs: Vec<Formula> },
    Implies { a: Box<Formula>, b: Box<Formula> },
}

impl Formula {
    pub fn atom(lhs: Expr, cmp: Cmp, rhs: Expr) -> Formula {
        Formula::Atom { lhs, cmp, rhs }
    }
    /// Parse a single comparison such as `P*L^3/(3*E*I) <= 0.02`.
    pub fn parse_atom(text: &str) -> Result<Formula> {
        for (op, cmp) in [
            ("<=", Cmp::Le),
            (">=", Cmp::Ge),
            ("==", Cmp::Eq),
            ("<", Cmp::Lt),
            (">", Cmp::Gt),
            ("=", Cmp::Eq),
        ] {
            if let Some((a, b)) = text.split_once(op) {
                return Ok(Formula::atom(Expr::parse(a)?, cmp, Expr::parse(b)?));
            }
        }
        Err(SimorphError::InvalidInput(format!(
            "no comparison operator in `{text}`"
        )))
    }
    pub fn and(fs: Vec<Formula>) -> Formula {
        Formula::And { fs }
    }
    pub fn or(fs: Vec<Formula>) -> Formula {
        Formula::Or { fs }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn not(f: Formula) -> Formula {
        Formula::Not { f: Box::new(f) }
    }
    pub fn implies(a: Formula, b: Formula) -> Formula {
        Formula::Implies {
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    pub fn to_smtlib(&self) -> Result<String> {
        self.to_smtlib_with(false)
    }

    pub fn to_smtlib_with(&self, il: bool) -> Result<String> {
        Ok(match self {
            Formula::True => "true".into(),
            Formula::False => "false".into(),
            Formula::BoolVar { name } => smt_symbol(name)?,
            Formula::Atom { lhs, cmp, rhs } => format!(
                "({} {} {})",
                cmp.smt(),
                lhs.to_smtlib_with(il)?,
                rhs.to_smtlib_with(il)?
            ),
            Formula::Not { f } => format!("(not {})", f.to_smtlib_with(il)?),
            Formula::And { fs } => {
                if fs.is_empty() {
                    "true".into()
                } else {
                    format!(
                        "(and {})",
                        fs.iter()
                            .map(|f| f.to_smtlib_with(il))
                            .collect::<Result<Vec<_>>>()?
                            .join(" ")
                    )
                }
            }
            Formula::Or { fs } => {
                if fs.is_empty() {
                    "false".into()
                } else {
                    format!(
                        "(or {})",
                        fs.iter()
                            .map(|f| f.to_smtlib_with(il))
                            .collect::<Result<Vec<_>>>()?
                            .join(" ")
                    )
                }
            }
            Formula::Implies { a, b } => {
                format!("(=> {} {})", a.to_smtlib_with(il)?, b.to_smtlib_with(il)?)
            }
        })
    }

    /// Native evaluation (Bool constants are read as 0/1 in `env`).
    pub fn eval(&self, env: &BTreeMap<String, f64>) -> Result<bool> {
        Ok(match self {
            Formula::True => true,
            Formula::False => false,
            Formula::BoolVar { name } => {
                env.get(name)
                    .copied()
                    .ok_or_else(|| SimorphError::InvalidInput(format!("unbound bool `{name}`")))?
                    != 0.0
            }
            Formula::Atom { lhs, cmp, rhs } => cmp.holds(lhs.eval(env)?, rhs.eval(env)?),
            Formula::Not { f } => !f.eval(env)?,
            Formula::And { fs } => {
                for f in fs {
                    if !f.eval(env)? {
                        return Ok(false);
                    }
                }
                true
            }
            Formula::Or { fs } => {
                for f in fs {
                    if f.eval(env)? {
                        return Ok(true);
                    }
                }
                false
            }
            Formula::Implies { a, b } => !a.eval(env)? || b.eval(env)?,
        })
    }

    /// Flatten nested conjunctions into atoms; `None` if the formula
    /// contains anything other than conjunctions of atoms.
    pub fn conjunct_atoms(&self) -> Option<Vec<(Expr, Cmp, Expr)>> {
        match self {
            Formula::True => Some(vec![]),
            Formula::Atom { lhs, cmp, rhs } => Some(vec![(lhs.clone(), *cmp, rhs.clone())]),
            Formula::And { fs } => {
                let mut out = vec![];
                for f in fs {
                    out.extend(f.conjunct_atoms()?);
                }
                Some(out)
            }
            _ => None,
        }
    }
}

/// A bounded verification problem: prove `goal` for every assignment of
/// the declared variables that lies inside `bounds` and satisfies all
/// `hypotheses`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Problem {
    pub name: String,
    /// Human-readable statement (goes into the proof artifact).
    pub statement: String,
    pub variables: Vec<(String, Sort)>,
    /// Closed bounds `[lo, hi]` per variable.
    pub bounds: BTreeMap<String, (f64, f64)>,
    pub hypotheses: Vec<Formula>,
    pub goal: Formula,
}

impl Problem {
    pub fn new(name: &str, statement: &str, goal: Formula) -> Self {
        Self {
            name: name.into(),
            statement: statement.into(),
            variables: vec![],
            bounds: BTreeMap::new(),
            hypotheses: vec![],
            goal,
        }
    }
    pub fn real(mut self, name: &str, lo: f64, hi: f64) -> Self {
        self.variables.push((name.into(), Sort::Real));
        self.bounds.insert(name.into(), (lo, hi));
        self
    }
    pub fn int(mut self, name: &str, lo: f64, hi: f64) -> Self {
        self.variables.push((name.into(), Sort::Int));
        self.bounds.insert(name.into(), (lo, hi));
        self
    }
    /// Unbounded real (only constrained by hypotheses).
    pub fn free_real(mut self, name: &str) -> Self {
        self.variables.push((name.into(), Sort::Real));
        self
    }
    pub fn boolean(mut self, name: &str) -> Self {
        self.variables.push((name.into(), Sort::Bool));
        self
    }
    pub fn hypothesis(mut self, f: Formula) -> Self {
        self.hypotheses.push(f);
        self
    }

    /// True when no variable is Real (integer literals are then used).
    pub fn int_only(&self) -> bool {
        !self.variables.iter().any(|(_, s)| *s == Sort::Real)
    }

    pub fn sort_of(&self, name: &str) -> Option<Sort> {
        self.variables
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| *s)
    }

    fn bound_atoms(&self) -> Result<Vec<String>> {
        let mut out = vec![];
        for (name, (lo, hi)) in &self.bounds {
            if self.sort_of(name).is_none() {
                return Err(SimorphError::InvalidInput(format!(
                    "bound on undeclared variable `{name}`"
                )));
            }
            if lo > hi {
                return Err(SimorphError::InvalidInput(format!(
                    "empty bound for `{name}`"
                )));
            }
            let int = self.sort_of(name) == Some(Sort::Int);
            let num = |v: f64| -> Result<String> {
                if int {
                    if v.fract() != 0.0 {
                        return Err(SimorphError::InvalidInput(format!(
                            "non-integer bound {v} on Int `{name}`"
                        )));
                    }
                    Ok(if v < 0.0 {
                        format!("(- {})", (-v) as i64)
                    } else {
                        format!("{}", v as i64)
                    })
                } else {
                    smt_number(v)
                }
            };
            out.push(format!("(>= {name} {})", num(*lo)?));
            out.push(format!("(<= {name} {})", num(*hi)?));
        }
        Ok(out)
    }

    /// SMT-LIB 2 script asserting bounds, hypotheses and the *negated*
    /// goal.  `unsat` ⇒ the goal is proved.
    pub fn to_smtlib_refutation(&self) -> Result<String> {
        let mut s = self.header()?;
        s.push_str(&format!(
            "(assert (not {}))\n",
            self.goal.to_smtlib_with(self.int_only())?
        ));
        s.push_str("(check-sat)\n(get-model)\n");
        Ok(s)
    }

    /// SMT-LIB 2 script asserting bounds, hypotheses **and** the goal:
    /// `sat` ⇒ the constraint set is feasible (with a witness).
    pub fn to_smtlib_feasibility(&self) -> Result<String> {
        let mut s = self.header()?;
        s.push_str(&format!(
            "(assert {})\n",
            self.goal.to_smtlib_with(self.int_only())?
        ));
        s.push_str("(check-sat)\n(get-model)\n");
        Ok(s)
    }

    fn header(&self) -> Result<String> {
        let mut s = format!(
            "; SIMORPH formal problem: {}\n; statement: {}\n",
            self.name,
            self.statement.replace('\n', " ")
        );
        let ints = self.variables.iter().any(|(_, s)| *s == Sort::Int);
        let reals = self.variables.iter().any(|(_, s)| *s == Sort::Real);
        let logic = match (ints, reals) {
            (true, true) => "ALL",
            (true, false) => "ALL",
            _ => "QF_NRA",
        };
        s.push_str(&format!("(set-logic {logic})\n"));
        for (n, sort) in &self.variables {
            s.push_str(&format!(
                "(declare-fun {} () {})\n",
                smt_symbol(n)?,
                sort.smt()
            ));
        }
        let b = self.bound_atoms()?;
        if !b.is_empty() {
            s.push_str(&format!("(assert (and {}))\n", b.join(" ")));
        }
        for h in &self.hypotheses {
            s.push_str(&format!(
                "(assert {})\n",
                h.to_smtlib_with(self.int_only())?
            ));
        }
        Ok(s)
    }

    /// Is `env` inside the bounds and does it satisfy every hypothesis?
    pub fn premises_hold(&self, env: &BTreeMap<String, f64>) -> Result<bool> {
        for (n, (lo, hi)) in &self.bounds {
            let v = *env
                .get(n)
                .ok_or_else(|| SimorphError::InvalidInput(format!("missing `{n}`")))?;
            if v < *lo || v > *hi {
                return Ok(false);
            }
        }
        for h in &self.hypotheses {
            if !h.eval(env)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtlib_script_shape() {
        let p = Problem::new(
            "beam",
            "deflection bound",
            Formula::parse_atom("P*L^3/(3*E*I) <= 0.02").unwrap(),
        )
        .real("P", 0.0, 1000.0)
        .real("L", 1.0, 2.0)
        .real("E", 2e11, 2.2e11)
        .real("I", 1e-5, 2e-5);
        let s = p.to_smtlib_refutation().unwrap();
        assert!(s.contains("(declare-fun P () Real)"));
        assert!(s.contains("(>= I 0.00001)"), "{s}");
        assert!(
            s.contains("(assert (not (<= (/ (* P (* L L L)) (* (* 3.0 E) I)) 0.02)))"),
            "{s}"
        );
        let env = BTreeMap::from([
            ("P".into(), 1.0),
            ("L".into(), 1.5),
            ("E".into(), 2.1e11),
            ("I".into(), 1.5e-5),
        ]);
        assert!(p.premises_hold(&env).unwrap());
        assert!(p.goal.eval(&env).unwrap());
    }
}
