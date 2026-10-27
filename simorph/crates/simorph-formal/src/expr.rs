//! Expression AST shared by every backend (SMT, SymPy, intervals, native
//! evaluation, dimensional analysis).
//!
//! User-facing text is parsed **once** into [`Expr`]; backends only ever
//! receive text that SIMORPH printed from the AST, so no untrusted string
//! reaches a solver or a Python parser.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use simorph_units::Dimension;
use std::collections::{BTreeMap, BTreeSet};

/// Elementary functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Func {
    Sqrt,
    Exp,
    Ln,
    Sin,
    Cos,
    Tan,
    Atan,
    Abs,
}

impl Func {
    pub fn name(self) -> &'static str {
        match self {
            Func::Sqrt => "sqrt",
            Func::Exp => "exp",
            Func::Ln => "ln",
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Atan => "atan",
            Func::Abs => "abs",
        }
    }
    fn from_name(s: &str) -> Option<Func> {
        Some(match s {
            "sqrt" => Func::Sqrt,
            "exp" => Func::Exp,
            "ln" | "log" => Func::Ln,
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "atan" => Func::Atan,
            "abs" => Func::Abs,
            _ => return None,
        })
    }
    fn sympy(self) -> &'static str {
        match self {
            Func::Ln => "log",
            Func::Abs => "Abs",
            other => other.name(),
        }
    }
}

/// Arithmetic expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Expr {
    /// Numeric constant (interpreted as its shortest decimal representation
    /// by exact backends).
    Num {
        v: f64,
    },
    /// The constant π (exact in SymPy; not expressible in SMT).
    Pi,
    Var {
        name: String,
    },
    Neg {
        a: Box<Expr>,
    },
    Add {
        a: Box<Expr>,
        b: Box<Expr>,
    },
    Sub {
        a: Box<Expr>,
        b: Box<Expr>,
    },
    Mul {
        a: Box<Expr>,
        b: Box<Expr>,
    },
    Div {
        a: Box<Expr>,
        b: Box<Expr>,
    },
    /// Power with a constant exponent (integers or simple rationals such as 1/2, 3/2).
    Pow {
        a: Box<Expr>,
        p: i32,
        q: i32,
    },
    Call {
        f: Func,
        a: Box<Expr>,
    },
}

#[allow(clippy::should_implement_trait)]
impl Expr {
    pub fn num(v: f64) -> Expr {
        Expr::Num { v }
    }
    pub fn var(n: &str) -> Expr {
        Expr::Var {
            name: n.to_string(),
        }
    }
    pub fn add(a: Expr, b: Expr) -> Expr {
        Expr::Add {
            a: Box::new(a),
            b: Box::new(b),
        }
    }
    pub fn sub(a: Expr, b: Expr) -> Expr {
        Expr::Sub {
            a: Box::new(a),
            b: Box::new(b),
        }
    }
    pub fn mul(a: Expr, b: Expr) -> Expr {
        Expr::Mul {
            a: Box::new(a),
            b: Box::new(b),
        }
    }
    pub fn div(a: Expr, b: Expr) -> Expr {
        Expr::Div {
            a: Box::new(a),
            b: Box::new(b),
        }
    }
    pub fn neg(a: Expr) -> Expr {
        Expr::Neg { a: Box::new(a) }
    }
    pub fn pow(a: Expr, p: i32, q: i32) -> Expr {
        Expr::Pow {
            a: Box::new(a),
            p,
            q,
        }
    }
    pub fn call(f: Func, a: Expr) -> Expr {
        Expr::Call { f, a: Box::new(a) }
    }

    /// Parse a textual expression: numbers, identifiers, `+ - * / ^`
    /// (also `**`), parentheses, `pi`, and the functions of [`Func`].
    /// Exponents must be constant integers or a parenthesised ratio of
    /// integers such as `^(3/2)`; a decimal exponent like `^0.5` is accepted
    /// when it equals k/2.
    pub fn parse(text: &str) -> Result<Expr> {
        let toks = tokenize(text)?;
        let mut p = Parser { t: toks, i: 0 };
        let e = p.expr()?;
        if p.i != p.t.len() {
            return Err(SimorphError::InvalidInput(format!(
                "trailing input in expression `{text}`"
            )));
        }
        Ok(e)
    }

    /// Variables appearing in the expression.
    pub fn variables(&self) -> BTreeSet<String> {
        let mut s = BTreeSet::new();
        self.collect_vars(&mut s);
        s
    }
    fn collect_vars(&self, s: &mut BTreeSet<String>) {
        match self {
            Expr::Var { name } => {
                s.insert(name.clone());
            }
            Expr::Num { .. } | Expr::Pi => {}
            Expr::Neg { a } | Expr::Pow { a, .. } | Expr::Call { a, .. } => a.collect_vars(s),
            Expr::Add { a, b } | Expr::Sub { a, b } | Expr::Mul { a, b } | Expr::Div { a, b } => {
                a.collect_vars(s);
                b.collect_vars(s);
            }
        }
    }

    /// Native f64 evaluation.  Unknown variables are an error.
    pub fn eval(&self, env: &BTreeMap<String, f64>) -> Result<f64> {
        Ok(match self {
            Expr::Num { v } => *v,
            Expr::Pi => std::f64::consts::PI,
            Expr::Var { name } => *env
                .get(name)
                .ok_or_else(|| SimorphError::InvalidInput(format!("unbound variable `{name}`")))?,
            Expr::Neg { a } => -a.eval(env)?,
            Expr::Add { a, b } => a.eval(env)? + b.eval(env)?,
            Expr::Sub { a, b } => a.eval(env)? - b.eval(env)?,
            Expr::Mul { a, b } => a.eval(env)? * b.eval(env)?,
            Expr::Div { a, b } => a.eval(env)? / b.eval(env)?,
            Expr::Pow { a, p, q } => {
                let x = a.eval(env)?;
                if *q == 1 {
                    x.powi(*p)
                } else {
                    x.powf(*p as f64 / *q as f64)
                }
            }
            Expr::Call { f, a } => {
                let x = a.eval(env)?;
                match f {
                    Func::Sqrt => x.sqrt(),
                    Func::Exp => x.exp(),
                    Func::Ln => x.ln(),
                    Func::Sin => x.sin(),
                    Func::Cos => x.cos(),
                    Func::Tan => x.tan(),
                    Func::Atan => x.atan(),
                    Func::Abs => x.abs(),
                }
            }
        })
    }

    /// Substitute variables by expressions.
    pub fn substitute(&self, map: &BTreeMap<String, Expr>) -> Expr {
        match self {
            Expr::Var { name } => map.get(name).cloned().unwrap_or_else(|| self.clone()),
            Expr::Num { .. } | Expr::Pi => self.clone(),
            Expr::Neg { a } => Expr::neg(a.substitute(map)),
            Expr::Add { a, b } => Expr::add(a.substitute(map), b.substitute(map)),
            Expr::Sub { a, b } => Expr::sub(a.substitute(map), b.substitute(map)),
            Expr::Mul { a, b } => Expr::mul(a.substitute(map), b.substitute(map)),
            Expr::Div { a, b } => Expr::div(a.substitute(map), b.substitute(map)),
            Expr::Pow { a, p, q } => Expr::pow(a.substitute(map), *p, *q),
            Expr::Call { f, a } => Expr::call(*f, a.substitute(map)),
        }
    }

    /// SMT-LIB 2 term over `Real`/`Int`.  Transcendental functions, `π`,
    /// and non-integer powers are refused (they are not part of the
    /// decidable theories used for proofs).  Negative integer powers become
    /// divisions.
    pub fn to_smtlib(&self) -> Result<String> {
        self.to_smtlib_with(false)
    }

    /// As [`Expr::to_smtlib`]; with `int_literals`, integral constants are
    /// printed as integer numerals (for problems over `Int` only).
    pub fn to_smtlib_with(&self, int_literals: bool) -> Result<String> {
        let il = int_literals;
        Ok(match self {
            Expr::Num { v } if il && v.fract() == 0.0 && v.abs() < 9.0e15 => {
                if *v < 0.0 {
                    format!("(- {})", (-*v) as i64)
                } else {
                    format!("{}", *v as i64)
                }
            }
            Expr::Num { v } => smt_number(*v)?,
            Expr::Pi => return Err(SimorphError::NotImplemented("π is not representable in SMT-LIB real arithmetic; use the symbolic (SymPy) or interval backend".into())),
            Expr::Var { name } => smt_symbol(name)?,
            Expr::Neg { a } => format!("(- {})", a.to_smtlib_with(il)?),
            Expr::Add { a, b } => format!("(+ {} {})", a.to_smtlib_with(il)?, b.to_smtlib_with(il)?),
            Expr::Sub { a, b } => format!("(- {} {})", a.to_smtlib_with(il)?, b.to_smtlib_with(il)?),
            Expr::Mul { a, b } => format!("(* {} {})", a.to_smtlib_with(il)?, b.to_smtlib_with(il)?),
            Expr::Div { a, b } => format!("(/ {} {})", a.to_smtlib_with(il)?, b.to_smtlib_with(il)?),
            Expr::Pow { a, p, q } => {
                if *q != 1 {
                    return Err(SimorphError::NotImplemented(format!("fractional power {p}/{q} is not supported by the SMT backend")));
                }
                let base = a.to_smtlib_with(il)?;
                let n = p.unsigned_abs();
                let prod = match n {
                    0 => if il { "1".to_string() } else { "1.0".to_string() },
                    1 => base.clone(),
                    _ => format!("(* {})", vec![base.clone(); n as usize].join(" ")),
                };
                if *p < 0 {
                    format!("(/ {} {prod})", if il { "1" } else { "1.0" })
                } else {
                    prod
                }
            }
            Expr::Call { f, .. } => {
                if *f == Func::Abs {
                    let a = match self {
                        Expr::Call { a, .. } => a.to_smtlib_with(il)?,
                        _ => unreachable!(),
                    };
                    format!("(ite (>= {a} {z}) {a} (- {a}))", z = if il { "0" } else { "0.0" })
                } else {
                    return Err(SimorphError::NotImplemented(format!("function `{}` is not supported by the SMT backend (non-polynomial)", f.name())));
                }
            }
        })
    }

    /// SymPy source text (only identifiers, numbers, operators and whitelisted functions).
    pub fn to_sympy(&self) -> String {
        match self {
            Expr::Num { v } => {
                let s = decimal(*v);
                if *v < 0.0 {
                    format!("({s})")
                } else {
                    s
                }
            }
            Expr::Pi => "pi".into(),
            Expr::Var { name } => name.clone(),
            Expr::Neg { a } => format!("(-{})", a.to_sympy()),
            Expr::Add { a, b } => format!("({} + {})", a.to_sympy(), b.to_sympy()),
            Expr::Sub { a, b } => format!("({} - {})", a.to_sympy(), b.to_sympy()),
            Expr::Mul { a, b } => format!("({} * {})", a.to_sympy(), b.to_sympy()),
            Expr::Div { a, b } => format!("({} / {})", a.to_sympy(), b.to_sympy()),
            Expr::Pow { a, p, q } => {
                if *q == 1 {
                    format!("({})**({p})", a.to_sympy())
                } else {
                    format!("({})**(Rational({p}, {q}))", a.to_sympy())
                }
            }
            Expr::Call { f, a } => format!("{}({})", f.sympy(), a.to_sympy()),
        }
    }

    /// Linear form `Σ cᵢ xᵢ + c₀` if the expression is affine, else `None`.
    pub fn linear_form(&self) -> Option<(BTreeMap<String, f64>, f64)> {
        fn go(e: &Expr) -> Option<(BTreeMap<String, f64>, f64)> {
            match e {
                Expr::Num { v } => Some((BTreeMap::new(), *v)),
                Expr::Pi => Some((BTreeMap::new(), std::f64::consts::PI)),
                Expr::Var { name } => Some((BTreeMap::from([(name.clone(), 1.0)]), 0.0)),
                Expr::Neg { a } => {
                    let (m, c) = go(a)?;
                    Some((m.into_iter().map(|(k, v)| (k, -v)).collect(), -c))
                }
                Expr::Add { a, b } | Expr::Sub { a, b } => {
                    let s = if matches!(e, Expr::Sub { .. }) {
                        -1.0
                    } else {
                        1.0
                    };
                    let (mut m, c) = go(a)?;
                    let (m2, c2) = go(b)?;
                    for (k, v) in m2 {
                        *m.entry(k).or_insert(0.0) += s * v;
                    }
                    Some((m, c + s * c2))
                }
                Expr::Mul { a, b } => {
                    let (ma, ca) = go(a)?;
                    let (mb, cb) = go(b)?;
                    if ma.values().all(|v| *v == 0.0) {
                        Some((mb.into_iter().map(|(k, v)| (k, v * ca)).collect(), ca * cb))
                    } else if mb.values().all(|v| *v == 0.0) {
                        Some((ma.into_iter().map(|(k, v)| (k, v * cb)).collect(), ca * cb))
                    } else {
                        None
                    }
                }
                Expr::Div { a, b } => {
                    let (ma, ca) = go(a)?;
                    let (mb, cb) = go(b)?;
                    if mb.values().any(|v| *v != 0.0) || cb == 0.0 {
                        return None;
                    }
                    Some((ma.into_iter().map(|(k, v)| (k, v / cb)).collect(), ca / cb))
                }
                Expr::Pow { a, p, q } if *q == 1 && *p == 1 => go(a),
                Expr::Pow { a, p, q } if *q == 1 && *p == 0 => {
                    let _ = a;
                    Some((BTreeMap::new(), 1.0))
                }
                _ => None,
            }
        }
        let (m, c) = go(self)?;
        Some((m.into_iter().filter(|(_, v)| *v != 0.0).collect(), c))
    }

    /// Infer the physical dimension given the units of the variables
    /// (simorph-units expressions).  Sums require identical dimensions;
    /// function arguments must be dimensionless (except `sqrt`/`abs`).
    pub fn dimension(&self, units: &BTreeMap<String, String>) -> Result<Dimension> {
        Ok(match self {
            Expr::Num { .. } | Expr::Pi => Dimension::NONE,
            Expr::Var { name } => {
                let u = units.get(name).ok_or_else(|| {
                    SimorphError::InvalidInput(format!("no unit declared for `{name}`"))
                })?;
                simorph_units::parse_unit(u)?.dimension
            }
            Expr::Neg { a } => a.dimension(units)?,
            Expr::Add { a, b } | Expr::Sub { a, b } => {
                let (da, db) = (a.dimension(units)?, b.dimension(units)?);
                if da != db {
                    return Err(SimorphError::Dimensional(format!(
                        "cannot add [{da}] and [{db}] in `{}`",
                        self.to_sympy()
                    )));
                }
                da
            }
            Expr::Mul { a, b } => a.dimension(units)?.mul(b.dimension(units)?),
            Expr::Div { a, b } => a.dimension(units)?.div(b.dimension(units)?),
            Expr::Pow { a, p, q } => a.dimension(units)?.pow(*p as i16, *q as i16),
            Expr::Call { f, a } => {
                let d = a.dimension(units)?;
                match f {
                    Func::Sqrt => d.pow(1, 2),
                    Func::Abs => d,
                    _ => {
                        if !d.is_dimensionless() {
                            return Err(SimorphError::Dimensional(format!(
                                "argument of {} must be dimensionless, got [{d}]",
                                f.name()
                            )));
                        }
                        Dimension::NONE
                    }
                }
            }
        })
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sympy().replace("**", "^"))
    }
}

/// Shortest round-trip decimal without exponent (Rust `Display` of f64).
pub(crate) fn decimal(v: f64) -> String {
    let s = format!("{}", v.abs());
    let s = if s.contains('.') { s } else { format!("{s}.0") };
    if v < 0.0 {
        format!("-{s}")
    } else {
        s
    }
}

pub(crate) fn smt_number(v: f64) -> Result<String> {
    if !v.is_finite() {
        return Err(SimorphError::InvalidInput(format!(
            "non-finite constant {v} cannot be used in a proof"
        )));
    }
    let s = decimal(v.abs());
    Ok(if v < 0.0 { format!("(- {s})") } else { s })
}

pub(crate) fn smt_symbol(name: &str) -> Result<String> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.chars().next().unwrap().is_ascii_digit()
    {
        return Err(SimorphError::InvalidInput(format!(
            "invalid symbol name `{name}`"
        )));
    }
    Ok(name.to_string())
}

// ------------------------------------------------------------------ parser

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(char),
}

fn tokenize(s: &str) -> Result<Vec<Tok>> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && i + 1 < c.len() && c[i + 1].is_ascii_digit())
        {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len()
                && (c[i] == 'e' || c[i] == 'E')
                && i + 1 < c.len()
                && (c[i + 1].is_ascii_digit()
                    || ((c[i + 1] == '-' || c[i + 1] == '+')
                        && i + 2 < c.len()
                        && c[i + 2].is_ascii_digit()))
            {
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| {
                SimorphError::InvalidInput(format!("bad number `{t}`"))
            })?));
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(c[st..i].iter().collect()));
        } else if ch == '*' && i + 1 < c.len() && c[i + 1] == '*' {
            out.push(Tok::Op('^'));
            i += 2;
        } else if "+-*/^(),".contains(ch) {
            out.push(Tok::Op(ch));
            i += 1;
        } else if ch == 'π' {
            out.push(Tok::Id("pi".into()));
            i += 1;
        } else {
            return Err(SimorphError::InvalidInput(format!(
                "unexpected character `{ch}` in expression"
            )));
        }
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(&Tok::Op(ch)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self) -> Result<Expr> {
        let mut e = self.term()?;
        loop {
            if self.eat('+') {
                e = Expr::add(e, self.term()?);
            } else if self.eat('-') {
                e = Expr::sub(e, self.term()?);
            } else {
                return Ok(e);
            }
        }
    }
    fn term(&mut self) -> Result<Expr> {
        let mut e = self.unary()?;
        loop {
            if self.eat('*') {
                e = Expr::mul(e, self.unary()?);
            } else if self.eat('/') {
                e = Expr::div(e, self.unary()?);
            } else {
                return Ok(e);
            }
        }
    }
    fn unary(&mut self) -> Result<Expr> {
        if self.eat('-') {
            return Ok(Expr::neg(self.unary()?));
        }
        if self.eat('+') {
            return self.unary();
        }
        self.power()
    }
    fn power(&mut self) -> Result<Expr> {
        let base = self.atom()?;
        if self.eat('^') {
            let (p, q) = self.exponent()?;
            return Ok(Expr::pow(base, p, q));
        }
        Ok(base)
    }
    fn exponent(&mut self) -> Result<(i32, i32)> {
        let neg = self.eat('-');
        let (p, q) = match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.i += 1;
                ratio(v)?
            }
            Some(Tok::Op('(')) => {
                self.i += 1;
                let n1 = self.exponent_number()?;
                let r = if self.eat('/') {
                    let n2 = self.exponent_number()?;
                    (n1, n2)
                } else {
                    (n1, 1)
                };
                if !self.eat(')') {
                    return Err(SimorphError::InvalidInput(
                        "expected `)` in exponent".into(),
                    ));
                }
                r
            }
            _ => {
                return Err(SimorphError::InvalidInput(
                    "exponent must be a constant".into(),
                ))
            }
        };
        Ok((if neg { -p } else { p }, q))
    }
    fn exponent_number(&mut self) -> Result<i32> {
        let neg = self.eat('-');
        match self.peek().cloned() {
            Some(Tok::Num(v)) if v.fract() == 0.0 => {
                self.i += 1;
                Ok(if neg { -(v as i32) } else { v as i32 })
            }
            _ => Err(SimorphError::InvalidInput(
                "exponent ratio must use integers".into(),
            )),
        }
    }
    fn atom(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.i += 1;
                Ok(Expr::num(v))
            }
            Some(Tok::Id(id)) => {
                self.i += 1;
                if id == "pi" {
                    return Ok(Expr::Pi);
                }
                if let Some(f) = Func::from_name(&id) {
                    if !self.eat('(') {
                        return Err(SimorphError::InvalidInput(format!(
                            "expected `(` after {id}"
                        )));
                    }
                    let a = self.expr()?;
                    if !self.eat(')') {
                        return Err(SimorphError::InvalidInput(format!(
                            "expected `)` after argument of {id}"
                        )));
                    }
                    return Ok(Expr::call(f, a));
                }
                Ok(Expr::var(&id))
            }
            Some(Tok::Op('(')) => {
                self.i += 1;
                let e = self.expr()?;
                if !self.eat(')') {
                    return Err(SimorphError::InvalidInput("expected `)`".into()));
                }
                Ok(e)
            }
            other => Err(SimorphError::InvalidInput(format!(
                "unexpected token {other:?}"
            ))),
        }
    }
}

fn ratio(v: f64) -> Result<(i32, i32)> {
    for q in [1, 2, 3, 4] {
        let p = v * q as f64;
        if (p - p.round()).abs() < 1e-12 {
            return Ok((p.round() as i32, q));
        }
    }
    Err(SimorphError::InvalidInput(format!(
        "unsupported exponent {v}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_eval_print_round_trip() {
        let e = Expr::parse("P*L^3/(3*E*I)").unwrap();
        let env = BTreeMap::from([
            ("P".into(), 1000.0),
            ("L".into(), 2.0),
            ("E".into(), 2e11),
            ("I".into(), 1e-5),
        ]);
        assert!((e.eval(&env).unwrap() - 8000.0 / 6e6).abs() < 1e-15);
        let back = Expr::parse(&e.to_sympy().replace("**", "^")).unwrap();
        assert_eq!(back.eval(&env).unwrap(), e.eval(&env).unwrap());
        assert_eq!(e.variables().len(), 4);
        let s = e.to_smtlib().unwrap();
        assert!(s.contains("(* L L L)"), "{s}");
        let k = Expr::parse("2*pi*sqrt(a^3/mu)").unwrap();
        assert!(k.to_smtlib().is_err());
        assert_eq!(
            Expr::parse("x^0.5").unwrap(),
            Expr::pow(Expr::var("x"), 1, 2)
        );
        assert!(Expr::parse("x^y").is_err());
        assert!(Expr::parse("__import__(1)").is_err());
        assert!(Expr::parse("x; import os").is_err());
    }

    #[test]
    fn linear_forms_and_dimensions() {
        let e = Expr::parse("0.5*x1 + 0.4*x2 - 3/2").unwrap();
        let (m, c) = e.linear_form().unwrap();
        assert_eq!(m["x1"], 0.5);
        assert_eq!(m["x2"], 0.4);
        assert_eq!(c, -1.5);
        assert!(Expr::parse("x*y").unwrap().linear_form().is_none());
        let units = BTreeMap::from([
            ("P".into(), "N".into()),
            ("L".into(), "m".into()),
            ("E".into(), "Pa".into()),
            ("I".into(), "m^4".into()),
        ]);
        let d = Expr::parse("P*L^3/(3*E*I)")
            .unwrap()
            .dimension(&units)
            .unwrap();
        assert_eq!(d, simorph_units::Dimension::LENGTH);
        assert!(Expr::parse("P + L").unwrap().dimension(&units).is_err());
        assert!(Expr::parse("exp(L)").unwrap().dimension(&units).is_err());
    }
}
