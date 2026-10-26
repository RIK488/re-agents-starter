//! Safe expression evaluator for invariants and verification criteria.
//!
//! Grammar (no assignment, no side effects, no function definitions):
//!
//! ```text
//! expr    := or
//! or      := and ( ("||" | "or") and )*
//! and     := cmp ( ("&&" | "and") cmp )*
//! cmp     := sum ( ("<" | "<=" | ">" | ">=" | "==" | "!=") sum )?
//! sum     := prod ( ("+" | "-") prod )*
//! prod    := unary ( ("*" | "/") unary )*
//! unary   := ("-" | "!" | "not") unary | pow
//! pow     := atom ( "^" unary )?
//! atom    := number | ident | ident "(" args ")" | "(" expr ")"
//! ```
//!
//! Functions: `abs, sqrt, exp, ln, log10, sin, cos, tan, min, max, floor,
//! ceil, round, sign`.  Constants: `pi, e, inf, nan, true, false`.

use indexmap::IndexMap;
use simorph_core::{Result, SimorphError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
}

impl Value {
    pub fn as_f64(self) -> Result<f64> {
        match self {
            Value::Num(x) => Ok(x),
            Value::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
        }
    }
    pub fn as_bool(self) -> Result<bool> {
        match self {
            Value::Bool(b) => Ok(b),
            Value::Num(x) => Ok(x != 0.0 && !x.is_nan()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(&'static str),
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
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Ident(c[st..i].iter().collect()));
            continue;
        }
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        let op2 = match two.as_str() {
            "<=" => Some("<="),
            ">=" => Some(">="),
            "==" => Some("=="),
            "!=" => Some("!="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            _ => None,
        };
        if let Some(op) = op2 {
            out.push(Tok::Op(op));
            i += 2;
            continue;
        }
        let op1 = match ch {
            '+' => Some("+"),
            '-' => Some("-"),
            '*' => Some("*"),
            '/' => Some("/"),
            '^' => Some("^"),
            '<' => Some("<"),
            '>' => Some(">"),
            '!' => Some("!"),
            _ => None,
        };
        if let Some(op) = op1 {
            out.push(Tok::Op(op));
            i += 1;
            continue;
        }
        match ch {
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

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    vars: &'a IndexMap<String, f64>,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let x = self.t.get(self.i).cloned();
        self.i += 1;
        x
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }
    fn is_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == name)
    }
    fn or(&mut self) -> Result<Value> {
        let mut l = self.and()?;
        while self.is_op("||") || self.is_ident("or") {
            self.next();
            let r = self.and()?;
            l = Value::Bool(l.as_bool()? || r.as_bool()?);
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<Value> {
        let mut l = self.cmp()?;
        while self.is_op("&&") || self.is_ident("and") {
            self.next();
            let r = self.cmp()?;
            l = Value::Bool(l.as_bool()? && r.as_bool()?);
        }
        Ok(l)
    }
    fn cmp(&mut self) -> Result<Value> {
        let l = self.sum()?;
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            if matches!(op, "<" | "<=" | ">" | ">=" | "==" | "!=") {
                self.next();
                let r = self.sum()?;
                let (a, b) = (l.as_f64()?, r.as_f64()?);
                return Ok(Value::Bool(match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    ">=" => a >= b,
                    "==" => a == b,
                    _ => a != b,
                }));
            }
        }
        Ok(l)
    }
    fn sum(&mut self) -> Result<Value> {
        let mut l = self.prod()?;
        loop {
            if self.is_op("+") {
                self.next();
                l = Value::Num(l.as_f64()? + self.prod()?.as_f64()?);
            } else if self.is_op("-") {
                self.next();
                l = Value::Num(l.as_f64()? - self.prod()?.as_f64()?);
            } else {
                return Ok(l);
            }
        }
    }
    fn prod(&mut self) -> Result<Value> {
        let mut l = self.unary()?;
        loop {
            if self.is_op("*") {
                self.next();
                l = Value::Num(l.as_f64()? * self.unary()?.as_f64()?);
            } else if self.is_op("/") {
                self.next();
                l = Value::Num(l.as_f64()? / self.unary()?.as_f64()?);
            } else {
                return Ok(l);
            }
        }
    }
    fn unary(&mut self) -> Result<Value> {
        if self.is_op("-") {
            self.next();
            return Ok(Value::Num(-self.unary()?.as_f64()?));
        }
        if self.is_op("!") || self.is_ident("not") {
            self.next();
            return Ok(Value::Bool(!self.unary()?.as_bool()?));
        }
        self.pow()
    }
    fn pow(&mut self) -> Result<Value> {
        let base = self.atom()?;
        if self.is_op("^") {
            self.next();
            let e = self.unary()?;
            return Ok(Value::Num(base.as_f64()?.powf(e.as_f64()?)));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<Value> {
        match self.next() {
            Some(Tok::Num(x)) => Ok(Value::Num(x)),
            Some(Tok::LParen) => {
                let v = self.or()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err(SimorphError::InvalidInput("expected `)`".into())),
                }
            }
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.next();
                    let mut args = vec![];
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.or()?.as_f64()?);
                            match self.next() {
                                Some(Tok::Comma) => continue,
                                Some(Tok::RParen) => break,
                                _ => {
                                    return Err(SimorphError::InvalidInput(
                                        "expected `,` or `)`".into(),
                                    ))
                                }
                            }
                        }
                    } else {
                        self.next();
                    }
                    return call(&name, &args);
                }
                match name.as_str() {
                    "pi" => Ok(Value::Num(std::f64::consts::PI)),
                    "e" => Ok(Value::Num(std::f64::consts::E)),
                    "inf" => Ok(Value::Num(f64::INFINITY)),
                    "nan" => Ok(Value::Num(f64::NAN)),
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    _ => self.vars.get(&name).map(|v| Value::Num(*v)).ok_or_else(|| {
                        SimorphError::NotFound(format!("variable `{name}` in expression"))
                    }),
                }
            }
            other => Err(SimorphError::InvalidInput(format!(
                "unexpected token {other:?}"
            ))),
        }
    }
}

fn call(name: &str, a: &[f64]) -> Result<Value> {
    let one = |f: fn(f64) -> f64| -> Result<Value> {
        if a.len() != 1 {
            return Err(SimorphError::InvalidInput(format!(
                "`{name}` takes one argument"
            )));
        }
        Ok(Value::Num(f(a[0])))
    };
    match name {
        "abs" => one(f64::abs),
        "sqrt" => one(f64::sqrt),
        "exp" => one(f64::exp),
        "ln" => one(f64::ln),
        "log10" => one(f64::log10),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "round" => one(f64::round),
        "sign" => one(f64::signum),
        "min" => {
            if a.is_empty() {
                return Err(SimorphError::InvalidInput("`min` needs arguments".into()));
            }
            Ok(Value::Num(a.iter().cloned().fold(f64::INFINITY, f64::min)))
        }
        "max" => {
            if a.is_empty() {
                return Err(SimorphError::InvalidInput("`max` needs arguments".into()));
            }
            Ok(Value::Num(
                a.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            ))
        }
        _ => Err(SimorphError::NotFound(format!("function `{name}`"))),
    }
}

/// Evaluate an expression with the given variables.
pub fn evaluate(expr: &str, vars: &IndexMap<String, f64>) -> Result<Value> {
    let t = tokenize(expr)?;
    if t.is_empty() {
        return Err(SimorphError::InvalidInput("empty expression".into()));
    }
    let mut p = P { t, i: 0, vars };
    let v = p.or()?;
    if p.i != p.t.len() {
        return Err(SimorphError::InvalidInput(format!(
            "trailing tokens in `{expr}`"
        )));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> IndexMap<String, f64> {
        [("energy_drift", -2e-7), ("T", 300.0), ("soc", 0.4)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    #[test]
    fn arithmetic_and_precedence() {
        let v = vars();
        assert_eq!(evaluate("1 + 2 * 3", &v).unwrap(), Value::Num(7.0));
        assert_eq!(evaluate("(1 + 2) * 3", &v).unwrap(), Value::Num(9.0));
        assert_eq!(evaluate("2 ^ 3 ^ 1", &v).unwrap(), Value::Num(8.0));
        assert_eq!(evaluate("-2 ^ 2", &v).unwrap(), Value::Num(-4.0));
        assert_eq!(evaluate("10 / 4", &v).unwrap(), Value::Num(2.5));
        assert_eq!(
            evaluate("max(1, 5, 3) - min(2, 0.5)", &v).unwrap(),
            Value::Num(4.5)
        );
        assert_eq!(evaluate("1e-3 * 1000", &v).unwrap(), Value::Num(1.0));
    }

    #[test]
    fn invariants() {
        let v = vars();
        assert_eq!(
            evaluate("abs(energy_drift) < 1e-6", &v).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate("T > 273.15 and T < 320", &v).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate("soc >= 0 && soc <= 1 && !(T < 0)", &v).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate("abs(energy_drift) < 1e-8 || soc > 0.5", &v).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            evaluate("sqrt(T) == T ^ 0.5", &v).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn errors() {
        let v = vars();
        assert!(evaluate("unknown + 1", &v).is_err());
        assert!(evaluate("1 +", &v).is_err());
        assert!(evaluate("foo(1)", &v).is_err());
        assert!(evaluate("1 = 2", &v).is_err());
        assert!(evaluate("", &v).is_err());
    }
}
