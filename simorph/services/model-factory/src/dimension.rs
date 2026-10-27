//! Dimensional inference over arithmetic expressions.
//!
//! Grammar (same operators as the validation evaluator, arithmetic subset):
//! `sum := prod (('+'|'-') prod)*`, `prod := unary (('*'|'/') unary)*`,
//! `unary := '-' unary | pow`, `pow := atom ('^' unary)?`,
//! `atom := number | ident | ident '(' args ')' | '(' sum ')'`.
//!
//! Rules: `+`/`-` require identical dimensions; `*`/`/` combine; `^`
//! requires a numeric literal exponent (integer or half-integer);
//! `exp/ln/log10/sin/cos/tan` require a dimensionless argument and return
//! dimensionless; `sqrt` halves exponents; `abs/floor/ceil/round/sign/min/max`
//! pass the dimension through (all arguments must agree).

use indexmap::IndexMap;
use simorph_core::{Result, SimorphError};
use simorph_units::{parse_unit, Dimension};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Result<Vec<Tok>> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < c.len() && c[i + 1].is_ascii_digit()) {
            let st = i;
            while i < c.len()
                && (c[i].is_ascii_digit()
                    || c[i] == '.'
                    || c[i] == 'e'
                    || c[i] == 'E'
                    || ((c[i] == '-' || c[i] == '+') && matches!(c[i - 1], 'e' | 'E')))
            {
                i += 1;
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| {
                SimorphError::InvalidInput(format!("bad number `{t}`"))
            })?));
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(c[st..i].iter().collect()));
            continue;
        }
        match ch {
            '+' | '-' | '*' | '/' | '^' => out.push(Tok::Op(ch)),
            '(' => out.push(Tok::LParen),
            ')' => out.push(Tok::RParen),
            ',' => out.push(Tok::Comma),
            _ => {
                return Err(SimorphError::InvalidInput(format!(
                    "unexpected character `{ch}` in expression"
                )))
            }
        }
        i += 1;
    }
    Ok(out)
}

/// Identifiers used by an expression (excluding function names and constants).
pub fn symbols(expr: &str) -> Result<BTreeSet<String>> {
    let toks = tokenize(expr)?;
    let mut out = BTreeSet::new();
    for (i, t) in toks.iter().enumerate() {
        if let Tok::Ident(name) = t {
            let is_call = matches!(toks.get(i + 1), Some(Tok::LParen));
            if !is_call && !is_constant(name) {
                out.insert(name.clone());
            }
        }
    }
    Ok(out)
}

fn is_constant(name: &str) -> bool {
    matches!(name, "pi" | "e" | "inf" | "nan" | "true" | "false")
}

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    units: &'a IndexMap<String, String>,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn sum(&mut self) -> Result<Dimension> {
        let d = self.prod()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.next();
            let r = self.prod()?;
            if r != d {
                return Err(SimorphError::Dimensional(format!(
                    "`{op}` between [{d}] and [{r}]"
                )));
            }
        }
        Ok(d)
    }
    fn prod(&mut self) -> Result<Dimension> {
        let mut d = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.next();
            let r = self.unary()?;
            d = if op == '*' { d.mul(r) } else { d.div(r) };
        }
        Ok(d)
    }
    fn unary(&mut self) -> Result<Dimension> {
        if let Some(Tok::Op('-')) = self.peek() {
            self.next();
            return self.unary();
        }
        self.pow()
    }
    fn pow(&mut self) -> Result<Dimension> {
        let base = self.atom()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.next();
            // exponent must be a numeric literal (optionally negated / parenthesised)
            let save = self.i;
            let exp = self.literal_exponent().ok_or_else(|| {
                self.i = save;
                SimorphError::Dimensional(
                    "exponent must be a numeric literal for dimensional analysis".into(),
                )
            })?;
            if base.is_dimensionless() {
                return Ok(Dimension::NONE);
            }
            let twice = exp * 2.0;
            if (twice - twice.round()).abs() > 1e-12 {
                return Err(SimorphError::Dimensional(format!(
                    "non half-integer exponent {exp} on a dimensioned quantity"
                )));
            }
            return Ok(base.pow(twice.round() as i16, 2));
        }
        Ok(base)
    }
    fn literal_exponent(&mut self) -> Option<f64> {
        let mut sign = 1.0;
        let mut parens = 0;
        loop {
            match self.peek() {
                Some(Tok::Op('-')) => {
                    sign = -sign;
                    self.next();
                }
                Some(Tok::LParen) => {
                    parens += 1;
                    self.next();
                }
                _ => break,
            }
        }
        let v = match self.next() {
            Some(Tok::Num(v)) => v,
            _ => return None,
        };
        for _ in 0..parens {
            match self.next() {
                Some(Tok::RParen) => {}
                _ => return None,
            }
        }
        Some(sign * v)
    }
    fn atom(&mut self) -> Result<Dimension> {
        match self.next() {
            Some(Tok::Num(_)) => Ok(Dimension::NONE),
            Some(Tok::LParen) => {
                let d = self.sum()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(d),
                    _ => Err(SimorphError::InvalidInput("expected `)`".into())),
                }
            }
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.next();
                    let mut args = vec![];
                    if let Some(Tok::RParen) = self.peek() {
                        self.next();
                    } else {
                        loop {
                            args.push(self.sum()?);
                            match self.next() {
                                Some(Tok::Comma) => continue,
                                Some(Tok::RParen) => break,
                                _ => {
                                    return Err(SimorphError::InvalidInput(format!(
                                        "bad argument list of `{name}`"
                                    )))
                                }
                            }
                        }
                    }
                    return self.function(&name, &args);
                }
                if is_constant(&name) {
                    return Ok(Dimension::NONE);
                }
                let u = self.units.get(&name).ok_or_else(|| {
                    SimorphError::Dimensional(format!("no unit declared for symbol `{name}`"))
                })?;
                Ok(parse_unit(u)?.dimension)
            }
            other => Err(SimorphError::InvalidInput(format!(
                "unexpected token {other:?}"
            ))),
        }
    }
    fn function(&self, name: &str, args: &[Dimension]) -> Result<Dimension> {
        match name {
            "exp" | "ln" | "log10" | "sin" | "cos" | "tan" => {
                let a = args.first().ok_or_else(|| {
                    SimorphError::InvalidInput(format!("`{name}` needs an argument"))
                })?;
                if !a.is_dimensionless() {
                    return Err(SimorphError::Dimensional(format!(
                        "argument of `{name}` has dimension [{a}], must be dimensionless"
                    )));
                }
                Ok(Dimension::NONE)
            }
            "sqrt" => {
                let a = args
                    .first()
                    .ok_or_else(|| SimorphError::InvalidInput("`sqrt` needs an argument".into()))?;
                Ok(a.pow(1, 2))
            }
            "abs" | "floor" | "ceil" | "round" | "sign" | "min" | "max" => {
                let first = args.first().ok_or_else(|| {
                    SimorphError::InvalidInput(format!("`{name}` needs an argument"))
                })?;
                if args.iter().any(|a| a != first) {
                    return Err(SimorphError::Dimensional(format!(
                        "arguments of `{name}` have different dimensions"
                    )));
                }
                Ok(if name == "sign" {
                    Dimension::NONE
                } else {
                    *first
                })
            }
            _ => Err(SimorphError::NotFound(format!("function `{name}`"))),
        }
    }
}

/// Infer the dimension of an arithmetic expression given symbol units.
pub fn infer_dimension(expr: &str, units: &IndexMap<String, String>) -> Result<Dimension> {
    let t = tokenize(expr)?;
    if t.is_empty() {
        return Err(SimorphError::InvalidInput("empty expression".into()));
    }
    let mut p = P { t, i: 0, units };
    let d = p.sum()?;
    if p.i != p.t.len() {
        return Err(SimorphError::InvalidInput(format!(
            "trailing tokens in `{expr}`"
        )));
    }
    Ok(d)
}

/// Check that `expr` has the dimension of `expected_unit`.
pub fn check_expression_unit(
    expr: &str,
    expected_unit: &str,
    units: &IndexMap<String, String>,
) -> Result<()> {
    let d = infer_dimension(expr, units)?;
    let e = parse_unit(expected_unit)?.dimension;
    if d != e {
        return Err(SimorphError::Dimensional(format!(
            "`{expr}` has dimension [{d}] but [{e}] ({expected_unit}) is required"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> IndexMap<String, String> {
        [
            ("T", "K"),
            ("T_env", "K"),
            ("k", "W/K"),
            ("C", "J/K"),
            ("P", "W"),
            ("t", "s"),
            ("x", "m"),
            ("v", "m/s"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
    }

    #[test]
    fn infers_compound_expressions() {
        let u = units();
        check_expression_unit("(P - k*(T - T_env))/C", "K/s", &u).unwrap();
        check_expression_unit("v*t + 0.5*x", "m", &u).unwrap();
        check_expression_unit("sqrt(x^2)", "m", &u).unwrap();
        check_expression_unit("exp(-t/(C/k))*T", "K", &u).unwrap();
        check_expression_unit("x^-1", "1/m", &u).unwrap();
    }

    #[test]
    fn rejects_inconsistent() {
        let u = units();
        assert!(matches!(
            check_expression_unit("T + t", "K", &u),
            Err(SimorphError::Dimensional(_))
        ));
        assert!(matches!(
            check_expression_unit("exp(T)", "1", &u),
            Err(SimorphError::Dimensional(_))
        ));
        assert!(matches!(
            check_expression_unit("P/C", "K", &u),
            Err(SimorphError::Dimensional(_))
        ));
        assert!(matches!(
            check_expression_unit("unknown*x", "m", &u),
            Err(SimorphError::Dimensional(_))
        ));
    }

    #[test]
    fn symbols_exclude_functions_and_constants() {
        let s = symbols("exp(-k*t) + pi*x").unwrap();
        assert_eq!(s.into_iter().collect::<Vec<_>>(), vec!["k", "t", "x"]);
    }
}
