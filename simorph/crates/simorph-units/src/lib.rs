//! # simorph-units
//!
//! Dimensional analysis and unit conversion.
//!
//! * [`Dimension`] – exponents over the seven SI base dimensions.
//! * [`Unit`] – a named scale (and optional offset) over a dimension.
//! * [`parse_unit`] – parses expressions such as `kg*m/s^2`, `W/(m^2*K)`,
//!   `km/h`, `mAh`, `1` (dimensionless).
//! * [`convert`] – converts a value between dimensionally-compatible units.
//! * [`check_same_dimension`] – the primitive used by the validation engine.
//!
//! Every unit in the registry is traceable to the SI definition.  Values are
//! never silently coerced: an incompatible conversion is an error.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::fmt;

/// Exponents over the SI base dimensions, in the order
/// length (m), mass (kg), time (s), current (A), temperature (K),
/// amount (mol), luminous intensity (cd).
///
/// Exponents are stored as rationals with a common small denominator to
/// support fractional dimensions (e.g. `m^(1/2)` in fracture mechanics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Dimension {
    /// Numerators.
    pub num: [i16; 7],
    /// Common denominator (>= 1).
    pub den: i16,
}

impl Dimension {
    pub const NONE: Dimension = Dimension { num: [0; 7], den: 1 };
    pub const LENGTH: Dimension = Dimension { num: [1, 0, 0, 0, 0, 0, 0], den: 1 };
    pub const MASS: Dimension = Dimension { num: [0, 1, 0, 0, 0, 0, 0], den: 1 };
    pub const TIME: Dimension = Dimension { num: [0, 0, 1, 0, 0, 0, 0], den: 1 };
    pub const CURRENT: Dimension = Dimension { num: [0, 0, 0, 1, 0, 0, 0], den: 1 };
    pub const TEMPERATURE: Dimension = Dimension { num: [0, 0, 0, 0, 1, 0, 0], den: 1 };
    pub const AMOUNT: Dimension = Dimension { num: [0, 0, 0, 0, 0, 1, 0], den: 1 };
    pub const LUMINOSITY: Dimension = Dimension { num: [0, 0, 0, 0, 0, 0, 1], den: 1 };

    fn gcd(a: i16, b: i16) -> i16 {
        let (mut a, mut b) = (a.abs(), b.abs());
        while b != 0 {
            let t = a % b;
            a = b;
            b = t;
        }
        if a == 0 {
            1
        } else {
            a
        }
    }

    fn normalise(mut self) -> Self {
        let mut g = self.den;
        for n in self.num {
            g = Self::gcd(g, n);
        }
        if g > 1 {
            for n in &mut self.num {
                *n /= g;
            }
            self.den /= g;
        }
        self
    }

    pub fn mul(self, other: Dimension) -> Dimension {
        let den = self.den * other.den / Self::gcd(self.den, other.den);
        let mut num = [0i16; 7];
        for i in 0..7 {
            num[i] = self.num[i] * (den / self.den) + other.num[i] * (den / other.den);
        }
        Dimension { num, den }.normalise()
    }

    pub fn inv(self) -> Dimension {
        let mut num = self.num;
        for n in &mut num {
            *n = -*n;
        }
        Dimension { num, den: self.den }
    }

    pub fn div(self, other: Dimension) -> Dimension {
        self.mul(other.inv())
    }

    /// Raise to a rational power p/q.
    pub fn pow(self, p: i16, q: i16) -> Dimension {
        let mut num = self.num;
        for n in &mut num {
            *n *= p;
        }
        Dimension { num, den: self.den * q }.normalise()
    }

    pub fn is_dimensionless(&self) -> bool {
        self.num.iter().all(|&n| n == 0)
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 7] = ["L", "M", "T", "I", "Θ", "N", "J"];
        if self.is_dimensionless() {
            return write!(f, "1");
        }
        let mut first = true;
        for (i, &n) in self.num.iter().enumerate() {
            if n == 0 {
                continue;
            }
            if !first {
                write!(f, "·")?;
            }
            first = false;
            if self.den == 1 {
                if n == 1 {
                    write!(f, "{}", NAMES[i])?;
                } else {
                    write!(f, "{}^{}", NAMES[i], n)?;
                }
            } else {
                write!(f, "{}^({}/{})", NAMES[i], n, self.den)?;
            }
        }
        Ok(())
    }
}

/// A unit: a dimension, a multiplicative factor to SI, and an optional
/// additive offset (only for affine temperature scales used alone).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Unit {
    pub dimension: Dimension,
    /// value_in_SI = factor * value + offset
    pub factor: f64,
    pub offset: f64,
    /// Original expression as written.
    pub expr: String,
}

impl Unit {
    pub fn dimensionless() -> Self {
        Self { dimension: Dimension::NONE, factor: 1.0, offset: 0.0, expr: "1".into() }
    }
    pub fn to_si(&self, v: f64) -> f64 {
        self.factor * v + self.offset
    }
    pub fn from_si(&self, si: f64) -> f64 {
        (si - self.offset) / self.factor
    }
}

/// Base (unprefixed) units known to the registry.  `(symbol, factor, offset, dimension, allow_prefix)`.
fn base_unit(symbol: &str) -> Option<(f64, f64, Dimension, bool)> {
    use Dimension as D;
    let d = |l: i16, m: i16, t: i16, i: i16, th: i16, n: i16, j: i16| D { num: [l, m, t, i, th, n, j], den: 1 };
    Some(match symbol {
        // SI base
        "m" => (1.0, 0.0, D::LENGTH, true),
        "g" => (1e-3, 0.0, D::MASS, true),
        "s" => (1.0, 0.0, D::TIME, true),
        "A" => (1.0, 0.0, D::CURRENT, true),
        "K" => (1.0, 0.0, D::TEMPERATURE, true),
        "mol" => (1.0, 0.0, D::AMOUNT, true),
        "cd" => (1.0, 0.0, D::LUMINOSITY, true),
        // SI derived
        "Hz" => (1.0, 0.0, d(0, 0, -1, 0, 0, 0, 0), true),
        "N" => (1.0, 0.0, d(1, 1, -2, 0, 0, 0, 0), true),
        "Pa" => (1.0, 0.0, d(-1, 1, -2, 0, 0, 0, 0), true),
        "J" => (1.0, 0.0, d(2, 1, -2, 0, 0, 0, 0), true),
        "W" => (1.0, 0.0, d(2, 1, -3, 0, 0, 0, 0), true),
        "C" => (1.0, 0.0, d(0, 0, 1, 1, 0, 0, 0), true),
        "V" => (1.0, 0.0, d(2, 1, -3, -1, 0, 0, 0), true),
        "F" => (1.0, 0.0, d(-2, -1, 4, 2, 0, 0, 0), true),
        "ohm" | "Ω" => (1.0, 0.0, d(2, 1, -3, -2, 0, 0, 0), true),
        "S" => (1.0, 0.0, d(-2, -1, 3, 2, 0, 0, 0), true),
        "Wb" => (1.0, 0.0, d(2, 1, -2, -1, 0, 0, 0), true),
        "T" => (1.0, 0.0, d(0, 1, -2, -1, 0, 0, 0), true),
        "H" => (1.0, 0.0, d(2, 1, -2, -2, 0, 0, 0), true),
        "lm" => (1.0, 0.0, D::LUMINOSITY, true),
        "lx" => (1.0, 0.0, d(-2, 0, 0, 0, 0, 0, 1), true),
        "Bq" => (1.0, 0.0, d(0, 0, -1, 0, 0, 0, 0), true),
        "Gy" | "Sv" => (1.0, 0.0, d(2, 0, -2, 0, 0, 0, 0), true),
        "kat" => (1.0, 0.0, d(0, 0, -1, 0, 0, 1, 0), true),
        "rad" | "sr" => (1.0, 0.0, D::NONE, true),
        // affine temperatures (offset applies only when used alone)
        "degC" | "°C" => (1.0, 273.15, D::TEMPERATURE, false),
        "degF" | "°F" => (5.0 / 9.0, 459.67 * 5.0 / 9.0, D::TEMPERATURE, false),
        // accepted non-SI
        "min" => (60.0, 0.0, D::TIME, false),
        "h" => (3600.0, 0.0, D::TIME, false),
        "d" | "day" => (86400.0, 0.0, D::TIME, false),
        "yr" | "year" => (365.25 * 86400.0, 0.0, D::TIME, false),
        "deg" | "°" => (std::f64::consts::PI / 180.0, 0.0, D::NONE, false),
        "L" | "l" => (1e-3, 0.0, d(3, 0, 0, 0, 0, 0, 0), true),
        "t" | "tonne" => (1e3, 0.0, D::MASS, false),
        "eV" => (1.602_176_634e-19, 0.0, d(2, 1, -2, 0, 0, 0, 0), true),
        "bar" => (1e5, 0.0, d(-1, 1, -2, 0, 0, 0, 0), true),
        "atm" => (101_325.0, 0.0, d(-1, 1, -2, 0, 0, 0, 0), false),
        "au" | "AU" => (149_597_870_700.0, 0.0, D::LENGTH, false),
        "ly" => (9.460_730_472_580_8e15, 0.0, D::LENGTH, false),
        "pc" => (3.085_677_581_491_37e16, 0.0, D::LENGTH, true),
        "Ah" => (3600.0, 0.0, d(0, 0, 1, 1, 0, 0, 0), true),
        "Wh" => (3600.0, 0.0, d(2, 1, -2, 0, 0, 0, 0), true),
        "cal" => (4.184, 0.0, d(2, 1, -2, 0, 0, 0, 0), true),
        "in" | "inch" => (0.0254, 0.0, D::LENGTH, false),
        "ft" => (0.3048, 0.0, D::LENGTH, false),
        "mi" | "mile" => (1609.344, 0.0, D::LENGTH, false),
        "nmi" => (1852.0, 0.0, D::LENGTH, false),
        "lb" => (0.453_592_37, 0.0, D::MASS, false),
        "kt" | "knot" => (1852.0 / 3600.0, 0.0, d(1, 0, -1, 0, 0, 0, 0), false),
        "G" => (1e-4, 0.0, d(0, 1, -2, -1, 0, 0, 0), false),
        "dB" => (1.0, 0.0, D::NONE, false),
        "percent" | "%" => (0.01, 0.0, D::NONE, false),
        "ppm" => (1e-6, 0.0, D::NONE, false),
        "1" | "" => (1.0, 0.0, D::NONE, false),
        _ => return None,
    })
}

fn prefix(p: &str) -> Option<f64> {
    Some(match p {
        "Q" => 1e30,
        "R" => 1e27,
        "Y" => 1e24,
        "Z" => 1e21,
        "E" => 1e18,
        "P" => 1e15,
        "T" => 1e12,
        "G" => 1e9,
        "M" => 1e6,
        "k" => 1e3,
        "h" => 1e2,
        "da" => 1e1,
        "d" => 1e-1,
        "c" => 1e-2,
        "m" => 1e-3,
        "u" | "µ" | "μ" => 1e-6,
        "n" => 1e-9,
        "p" => 1e-12,
        "f" => 1e-15,
        "a" => 1e-18,
        "z" => 1e-21,
        "y" => 1e-24,
        "r" => 1e-27,
        "q" => 1e-30,
        _ => return None,
    })
}

/// Resolve a single symbol (possibly prefixed) into (factor, offset, dimension).
fn resolve_symbol(sym: &str) -> Result<(f64, f64, Dimension)> {
    if let Some((f, o, d, _)) = base_unit(sym) {
        return Ok((f, o, d));
    }
    // Try prefix + base, longest prefix first ("da").
    for plen in [2usize, 1] {
        if sym.chars().count() > plen {
            let (p, rest): (String, String) = {
                let mut it = sym.chars();
                let p: String = it.by_ref().take(plen).collect();
                (p, it.collect())
            };
            if let (Some(pf), Some((f, o, d, allow))) = (prefix(&p), base_unit(&rest)) {
                if allow {
                    return Ok((pf * f, o, d));
                }
            }
        }
    }
    Err(SimorphError::Unit(format!("unknown unit symbol `{sym}`")))
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Sym(String),
    Num(f64),
    Mul,
    Div,
    Pow,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Result<Vec<Tok>> {
    let mut out = vec![];
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '·' | '.' if !(c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) => {
                // whitespace or middle dot acts as multiplication when between symbols
                if matches!(out.last(), Some(Tok::Sym(_)) | Some(Tok::RParen) | Some(Tok::Num(_))) {
                    // look ahead for a symbol start
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == ' ' {
                        j += 1;
                    }
                    if j < chars.len() && (chars[j].is_alphabetic() || chars[j] == '(' || chars[j] == '°' || chars[j] == '%' || chars[j] == 'Ω' || chars[j] == 'µ' || chars[j] == 'μ') {
                        out.push(Tok::Mul);
                    }
                }
                i += 1;
            }
            '*' => {
                out.push(Tok::Mul);
                i += 1;
            }
            '/' => {
                out.push(Tok::Div);
                i += 1;
            }
            '^' => {
                out.push(Tok::Pow);
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '-' | '+' | '0'..='9' | '.' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || ((chars[i] == '-' || chars[i] == '+') && matches!(chars[i - 1], 'e' | 'E'))) {
                    i += 1;
                }
                let txt: String = chars[start..i].iter().collect();
                let v: f64 = txt
                    .parse()
                    .map_err(|_| SimorphError::Unit(format!("bad number `{txt}` in unit expression")))?;
                out.push(Tok::Num(v));
            }
            _ if c.is_alphabetic() || c == '°' || c == '%' || c == 'Ω' || c == 'µ' || c == 'μ' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '°' || chars[i] == '%' || chars[i] == 'Ω' || chars[i] == 'µ' || chars[i] == 'μ' || chars[i] == '_') {
                    i += 1;
                }
                let sym: String = chars[start..i].iter().collect();
                out.push(Tok::Sym(sym));
            }
            _ => return Err(SimorphError::Unit(format!("unexpected character `{c}` in unit expression `{s}`"))),
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Number of symbol atoms seen; offsets are only kept for lone symbols.
    atoms: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    // expr := term (('*'|'/') term)*
    fn expr(&mut self) -> Result<(f64, f64, Dimension)> {
        let mut acc = self.term()?;
        while let Some(t) = self.peek() {
            match t {
                Tok::Mul => {
                    self.next();
                    let r = self.term()?;
                    acc = (acc.0 * r.0, 0.0, acc.2.mul(r.2));
                }
                Tok::Div => {
                    self.next();
                    let r = self.term()?;
                    acc = (acc.0 / r.0, 0.0, acc.2.div(r.2));
                }
                _ => break,
            }
        }
        Ok(acc)
    }
    // term := atom ('^' exponent)?
    fn term(&mut self) -> Result<(f64, f64, Dimension)> {
        let a = self.atom()?;
        if let Some(Tok::Pow) = self.peek() {
            self.next();
            let (p, q) = self.exponent()?;
            let f = a.0.powf(p as f64 / q as f64);
            return Ok((f, 0.0, a.2.pow(p, q)));
        }
        Ok(a)
    }
    // exponent := int | '(' int '/' int ')' | '-' int
    fn exponent(&mut self) -> Result<(i16, i16)> {
        match self.next() {
            Some(Tok::Num(v)) => {
                if v.fract() == 0.0 {
                    Ok((v as i16, 1))
                } else {
                    // allow 0.5
                    let q = 2;
                    let p = (v * q as f64).round();
                    if (p / q as f64 - v).abs() < 1e-12 {
                        Ok((p as i16, q))
                    } else {
                        Err(SimorphError::Unit(format!("unsupported fractional exponent {v}")))
                    }
                }
            }
            Some(Tok::LParen) => {
                let p = match self.next() {
                    Some(Tok::Num(v)) => v as i16,
                    _ => return Err(SimorphError::Unit("bad exponent".into())),
                };
                let mut q = 1;
                if let Some(Tok::Div) = self.peek() {
                    self.next();
                    q = match self.next() {
                        Some(Tok::Num(v)) => v as i16,
                        _ => return Err(SimorphError::Unit("bad exponent".into())),
                    };
                }
                match self.next() {
                    Some(Tok::RParen) => Ok((p, q)),
                    _ => Err(SimorphError::Unit("expected `)` after exponent".into())),
                }
            }
            other => Err(SimorphError::Unit(format!("bad exponent token {other:?}"))),
        }
    }
    // atom := symbol | number | '(' expr ')'
    fn atom(&mut self) -> Result<(f64, f64, Dimension)> {
        match self.next() {
            Some(Tok::Sym(s)) => {
                self.atoms += 1;
                resolve_symbol(&s)
            }
            Some(Tok::Num(v)) => {
                self.atoms += 1;
                Ok((v, 0.0, Dimension::NONE))
            }
            Some(Tok::LParen) => {
                let e = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err(SimorphError::Unit("expected `)`".into())),
                }
            }
            other => Err(SimorphError::Unit(format!("unexpected token {other:?}"))),
        }
    }
}

/// Parse a unit expression.
pub fn parse_unit(expr: &str) -> Result<Unit> {
    let trimmed = expr.trim();
    if trimmed.is_empty() || trimmed == "1" || trimmed == "-" {
        return Ok(Unit::dimensionless());
    }
    let toks = tokenize(trimmed)?;
    let mut p = Parser { toks, pos: 0, atoms: 0 };
    let (factor, offset, dimension) = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(SimorphError::Unit(format!("trailing tokens in unit expression `{expr}`")));
    }
    // Offsets (°C, °F) only apply for a lone symbol; inside compounds they
    // denote temperature *differences*.
    let offset = if p.atoms == 1 { offset } else { 0.0 };
    Ok(Unit { dimension, factor, offset, expr: trimmed.to_string() })
}

/// Convert `value` from unit `from` to unit `to`.
pub fn convert(value: f64, from: &str, to: &str) -> Result<f64> {
    let f = parse_unit(from)?;
    let t = parse_unit(to)?;
    if f.dimension != t.dimension {
        return Err(SimorphError::Dimensional(format!(
            "cannot convert `{from}` [{}] to `{to}` [{}]",
            f.dimension, t.dimension
        )));
    }
    Ok(t.from_si(f.to_si(value)))
}

/// Return Ok(()) if both expressions have the same dimension.
pub fn check_same_dimension(a: &str, b: &str) -> Result<()> {
    let ua = parse_unit(a)?;
    let ub = parse_unit(b)?;
    if ua.dimension != ub.dimension {
        return Err(SimorphError::Dimensional(format!(
            "`{a}` [{}] and `{b}` [{}] have different dimensions",
            ua.dimension, ub.dimension
        )));
    }
    Ok(())
}

/// Dimension of a product of units (used to check equations such as
/// `P = U * I`): returns the dimension of `lhs` and of the product of
/// `rhs_factors`, and whether they agree.
pub fn check_product_equation(lhs: &str, rhs_factors: &[&str]) -> Result<bool> {
    let l = parse_unit(lhs)?;
    let mut d = Dimension::NONE;
    for f in rhs_factors {
        d = d.mul(parse_unit(f)?.dimension);
    }
    Ok(l.dimension == d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn parses_basic_and_prefixed() {
        assert_eq!(parse_unit("km").unwrap().factor, 1e3);
        assert_eq!(parse_unit("mm").unwrap().factor, 1e-3);
        assert_eq!(parse_unit("kg").unwrap().factor, 1.0);
        assert_eq!(parse_unit("MW").unwrap().factor, 1e6);
        assert_eq!(parse_unit("µm").unwrap().factor, 1e-6);
        assert!(close(parse_unit("mAh").unwrap().factor, 3.6));
    }

    #[test]
    fn compound_expressions() {
        let n = parse_unit("kg*m/s^2").unwrap();
        assert_eq!(n.dimension, parse_unit("N").unwrap().dimension);
        let w = parse_unit("J/s").unwrap();
        assert_eq!(w.dimension, parse_unit("W").unwrap().dimension);
        let h = parse_unit("W/(m^2*K)").unwrap();
        assert_eq!(h.dimension, parse_unit("kg/(s^3*K)").unwrap().dimension);
        let v = parse_unit("km/h").unwrap();
        assert!(close(v.factor, 1000.0 / 3600.0));
        let s = parse_unit("m s^-2").unwrap();
        assert_eq!(s.dimension, parse_unit("m/s^2").unwrap().dimension);
        let kic = parse_unit("MPa*m^0.5").unwrap();
        assert_eq!(kic.dimension, parse_unit("Pa*m^(1/2)").unwrap().dimension);
    }

    #[test]
    fn conversions() {
        assert!(close(convert(1.0, "km", "m").unwrap(), 1000.0));
        assert!(close(convert(1.0, "h", "s").unwrap(), 3600.0));
        assert!(close(convert(100.0, "degC", "K").unwrap(), 373.15));
        assert!(close(convert(0.0, "K", "degC").unwrap(), -273.15));
        assert!(close(convert(212.0, "degF", "degC").unwrap(), 100.0));
        assert!(close(convert(1.0, "kWh", "J").unwrap(), 3.6e6));
        assert!(close(convert(1.0, "eV", "J").unwrap(), 1.602176634e-19));
        assert!(close(convert(1.0, "au", "km").unwrap(), 149_597_870.7));
        // temperature differences in compounds have no offset
        assert!(close(convert(10.0, "W/degC", "W/K").unwrap(), 10.0));
    }

    #[test]
    fn incompatible_conversion_is_an_error() {
        assert!(convert(1.0, "m", "s").is_err());
        assert!(check_same_dimension("J", "W").is_err());
        assert!(check_same_dimension("J", "N*m").is_ok());
    }

    #[test]
    fn product_equation_check() {
        assert!(check_product_equation("W", &["V", "A"]).unwrap());
        assert!(check_product_equation("J", &["N", "m"]).unwrap());
        assert!(!check_product_equation("J", &["N", "s"]).unwrap());
        assert!(check_product_equation("1", &["rad"]).unwrap());
    }

    #[test]
    fn unknown_symbol_is_rejected() {
        assert!(parse_unit("furlong").is_err());
        assert!(parse_unit("kdegC").is_err());
    }
}
