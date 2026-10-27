//! Interval arithmetic with outward rounding.
//!
//! Soundness: `+ − × ÷` and `sqrt` are correctly rounded in IEEE 754, so
//! widening each result by one ulp (`next_down` / `next_up`) gives a
//! rigorous enclosure.  For `exp`, `ln`, `sin`, `cos`, `tan`, `atan` and
//! fractional powers the platform `libm` is not guaranteed to be correctly
//! rounded; results are widened by [`LIBM_ULPS`] ulps, which covers the
//! documented accuracy of glibc but is an **assumption**, reported through
//! [`Enclosure::rigorous`] = `false`.

#![allow(clippy::should_implement_trait)]

use crate::expr::{Expr, Func};
use crate::formula::Cmp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::collections::BTreeMap;

/// Widening applied to libm results.
pub const LIBM_ULPS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Interval {
    pub lo: f64,
    pub hi: f64,
}

fn down(v: f64, k: u32) -> f64 {
    let mut x = v;
    for _ in 0..k {
        x = x.next_down();
    }
    x
}
fn up(v: f64, k: u32) -> f64 {
    let mut x = v;
    for _ in 0..k {
        x = x.next_up();
    }
    x
}

impl Interval {
    pub const ENTIRE: Interval = Interval {
        lo: f64::NEG_INFINITY,
        hi: f64::INFINITY,
    };
    pub fn new(lo: f64, hi: f64) -> Self {
        assert!(lo <= hi, "empty interval [{lo}, {hi}]");
        Self { lo, hi }
    }
    pub fn point(v: f64) -> Self {
        Self { lo: v, hi: v }
    }
    /// Enclosure of a decimal constant whose nearest double is `v`.
    pub fn constant(v: f64) -> Self {
        Self {
            lo: down(v, 1),
            hi: up(v, 1),
        }
    }
    pub fn contains(&self, v: f64) -> bool {
        self.lo <= v && v <= self.hi
    }
    pub fn contains_zero(&self) -> bool {
        self.lo <= 0.0 && 0.0 <= self.hi
    }
    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }
    pub fn mid(&self) -> f64 {
        0.5 * (self.lo + self.hi)
    }
    fn widened(lo: f64, hi: f64, k: u32) -> Self {
        if lo.is_nan() || hi.is_nan() {
            return Interval::ENTIRE;
        }
        Self {
            lo: down(lo, k),
            hi: up(hi, k),
        }
    }
    pub fn add(self, o: Self) -> Self {
        Self::widened(self.lo + o.lo, self.hi + o.hi, 1)
    }
    pub fn sub(self, o: Self) -> Self {
        Self::widened(self.lo - o.hi, self.hi - o.lo, 1)
    }
    pub fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
    pub fn mul(self, o: Self) -> Self {
        let c = [
            self.lo * o.lo,
            self.lo * o.hi,
            self.hi * o.lo,
            self.hi * o.hi,
        ];
        // 0 * inf = NaN: treat as 0 contribution bound conservatively
        if c.iter().any(|x| x.is_nan()) {
            return Interval::ENTIRE;
        }
        let lo = c.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = c.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Self::widened(lo, hi, 1)
    }
    pub fn recip(self) -> Self {
        if self.contains_zero() {
            return Interval::ENTIRE;
        }
        Self::widened(1.0 / self.hi, 1.0 / self.lo, 1)
    }
    pub fn div(self, o: Self) -> Self {
        if o.contains_zero() {
            return Interval::ENTIRE;
        }
        let c = [
            self.lo / o.lo,
            self.lo / o.hi,
            self.hi / o.lo,
            self.hi / o.hi,
        ];
        if c.iter().any(|x| x.is_nan()) {
            return Interval::ENTIRE;
        }
        let lo = c.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = c.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Self::widened(lo, hi, 1)
    }
    /// Integer power by repeated squaring of the *absolute* value for even
    /// exponents (keeps the enclosure tight around 0).
    pub fn powi(self, n: i32) -> Self {
        if n == 0 {
            return Interval::point(1.0);
        }
        if n < 0 {
            return self.powi(-n).recip();
        }
        if n % 2 == 0 {
            let a = if self.contains_zero() {
                Interval {
                    lo: 0.0,
                    hi: self.lo.abs().max(self.hi.abs()),
                }
            } else if self.lo > 0.0 {
                self
            } else {
                self.neg()
            };
            let mut r = Interval::point(1.0);
            for _ in 0..n {
                r = r.mul(a);
            }
            Interval {
                lo: r.lo.max(0.0),
                hi: r.hi,
            }
        } else {
            // odd power is monotone
            let mut lo = Interval::point(self.lo);
            let mut hi = Interval::point(self.hi);
            let (bl, bh) = (lo, hi);
            for _ in 1..n {
                lo = lo.mul(bl);
                hi = hi.mul(bh);
            }
            Interval {
                lo: lo.lo,
                hi: hi.hi,
            }
        }
    }
    pub fn sqrt(self) -> Result<Self> {
        if self.hi < 0.0 {
            return Err(SimorphError::ValidityDomain(format!(
                "sqrt of negative interval [{}, {}]",
                self.lo, self.hi
            )));
        }
        let lo = self.lo.max(0.0).sqrt();
        Ok(Self {
            lo: down(lo, 1).max(0.0),
            hi: up(self.hi.sqrt(), 1),
        })
    }
    fn monotone(self, f: fn(f64) -> f64) -> Self {
        Self::widened(f(self.lo), f(self.hi), LIBM_ULPS)
    }
    fn sin_range(self, shift: f64) -> Self {
        // sin(x + shift); extrema at π/2 + kπ
        let (lo, hi) = (self.lo + shift, self.hi + shift);
        if !(lo.is_finite() && hi.is_finite()) || hi - lo >= 2.0 * std::f64::consts::PI {
            return Interval::new(-1.0, 1.0);
        }
        let mut a = lo.sin().min(hi.sin());
        let mut b = lo.sin().max(hi.sin());
        let half_pi = std::f64::consts::FRAC_PI_2;
        let two_pi = 2.0 * std::f64::consts::PI;
        let tol = 1e-12 * (1.0 + lo.abs().max(hi.abs()));
        // maxima at π/2 + 2kπ, minima at -π/2 + 2kπ (include near-boundary points conservatively)
        let k0 = ((lo - half_pi) / two_pi).floor() as i64 - 1;
        for k in k0..k0 + 4 {
            let pmax = half_pi + two_pi * k as f64;
            if pmax >= lo - tol && pmax <= hi + tol {
                b = 1.0;
            }
            let pmin = -half_pi + two_pi * k as f64;
            if pmin >= lo - tol && pmin <= hi + tol {
                a = -1.0;
            }
        }
        let w = Self::widened(a, b, LIBM_ULPS);
        Interval {
            lo: w.lo.max(-1.0),
            hi: w.hi.min(1.0),
        }
    }
}

/// Result of evaluating an expression over a box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Enclosure {
    pub range: Interval,
    /// True when only correctly-rounded IEEE operations were used.
    pub rigorous: bool,
}

/// Natural interval extension of `e` over the box `b`.
pub fn enclose(e: &Expr, b: &BTreeMap<String, Interval>) -> Result<Enclosure> {
    let mut rigorous = true;
    let r = eval(e, b, &mut rigorous)?;
    Ok(Enclosure { range: r, rigorous })
}

fn eval(e: &Expr, b: &BTreeMap<String, Interval>, rig: &mut bool) -> Result<Interval> {
    Ok(match e {
        Expr::Num { v } => Interval::constant(*v),
        Expr::Pi => Interval::constant(std::f64::consts::PI),
        Expr::Var { name } => *b
            .get(name)
            .ok_or_else(|| SimorphError::InvalidInput(format!("no interval for `{name}`")))?,
        Expr::Neg { a } => eval(a, b, rig)?.neg(),
        Expr::Add { a, b: c } => eval(a, b, rig)?.add(eval(c, b, rig)?),
        Expr::Sub { a, b: c } => eval(a, b, rig)?.sub(eval(c, b, rig)?),
        Expr::Mul { a, b: c } => {
            // x*x → square (tighter)
            if a == c {
                eval(a, b, rig)?.powi(2)
            } else {
                eval(a, b, rig)?.mul(eval(c, b, rig)?)
            }
        }
        Expr::Div { a, b: c } => eval(a, b, rig)?.div(eval(c, b, rig)?),
        Expr::Pow { a, p, q } => {
            let x = eval(a, b, rig)?;
            if *q == 1 {
                x.powi(*p)
            } else if *q == 2 && *p == 1 {
                x.sqrt()?
            } else {
                *rig = false;
                if x.hi < 0.0 {
                    return Err(SimorphError::ValidityDomain(
                        "fractional power of a negative interval".into(),
                    ));
                }
                let e = *p as f64 / *q as f64;
                let lo = x.lo.max(0.0);
                let (l, h) = if e >= 0.0 {
                    (lo.powf(e), x.hi.powf(e))
                } else {
                    (x.hi.powf(e), lo.powf(e))
                };
                Interval::widened(l, h, LIBM_ULPS)
            }
        }
        Expr::Call { f, a } => {
            let x = eval(a, b, rig)?;
            match f {
                Func::Sqrt => x.sqrt()?,
                Func::Abs => {
                    if x.contains_zero() {
                        Interval {
                            lo: 0.0,
                            hi: x.lo.abs().max(x.hi.abs()),
                        }
                    } else if x.lo > 0.0 {
                        x
                    } else {
                        x.neg()
                    }
                }
                Func::Exp => {
                    *rig = false;
                    let r = x.monotone(f64::exp);
                    Interval {
                        lo: r.lo.max(0.0),
                        hi: r.hi,
                    }
                }
                Func::Ln => {
                    *rig = false;
                    if x.hi <= 0.0 {
                        return Err(SimorphError::ValidityDomain(
                            "ln of non-positive interval".into(),
                        ));
                    }
                    if x.lo <= 0.0 {
                        Interval {
                            lo: f64::NEG_INFINITY,
                            hi: up(x.hi.ln(), LIBM_ULPS),
                        }
                    } else {
                        x.monotone(f64::ln)
                    }
                }
                Func::Atan => {
                    *rig = false;
                    x.monotone(f64::atan)
                }
                Func::Sin => {
                    *rig = false;
                    x.sin_range(0.0)
                }
                Func::Cos => {
                    *rig = false;
                    x.sin_range(std::f64::consts::FRAC_PI_2)
                }
                Func::Tan => {
                    *rig = false;
                    let half_pi = std::f64::consts::FRAC_PI_2;
                    let k_lo = ((x.lo - half_pi) / std::f64::consts::PI).floor();
                    let k_hi = ((x.hi - half_pi) / std::f64::consts::PI).floor();
                    if k_lo != k_hi || x.width() >= std::f64::consts::PI {
                        Interval::ENTIRE
                    } else {
                        x.monotone(f64::tan)
                    }
                }
            }
        }
    })
}

/// Outcome of a branch-and-bound bound check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BoundCheck {
    /// The atom holds on the whole box (all leaf boxes certified).
    Holds {
        boxes: usize,
        rigorous: bool,
        worst_margin: f64,
    },
    /// A sampled point violates the atom (native f64 evaluation).
    Violated {
        point: BTreeMap<String, f64>,
        value: f64,
    },
    /// Neither certified nor refuted within the box budget.
    Inconclusive { boxes: usize },
}

/// Check `g(x) cmp 0` for every x in `bbox` by interval branch-and-bound
/// (bisection of the widest variable, at most `max_boxes` leaves).
pub fn check_atom_on_box(
    g: &Expr,
    cmp: Cmp,
    bbox: &BTreeMap<String, Interval>,
    max_boxes: usize,
) -> Result<BoundCheck> {
    if cmp == Cmp::Eq {
        return Ok(BoundCheck::Inconclusive { boxes: 0 });
    }
    let mut stack = vec![bbox.clone()];
    let mut certified = 0usize;
    let mut rigorous = true;
    let mut worst = f64::INFINITY;
    let mut processed = 0usize;
    while let Some(bx) = stack.pop() {
        processed += 1;
        if processed > max_boxes {
            return Ok(BoundCheck::Inconclusive { boxes: processed });
        }
        let enc = enclose(g, &bx)?;
        rigorous &= enc.rigorous;
        let r = enc.range;
        let (ok, margin) = match cmp {
            Cmp::Le => (r.hi <= 0.0, -r.hi),
            Cmp::Lt => (r.hi < 0.0, -r.hi),
            Cmp::Ge => (r.lo >= 0.0, r.lo),
            Cmp::Gt => (r.lo > 0.0, r.lo),
            Cmp::Eq => unreachable!(),
        };
        if ok {
            certified += 1;
            worst = worst.min(margin);
            continue;
        }
        // try the midpoint: a genuine violation ends the search
        let mid: BTreeMap<String, f64> = bx.iter().map(|(k, v)| (k.clone(), v.mid())).collect();
        let gv = g.eval(&mid)?;
        if !cmp.holds(gv, 0.0) {
            return Ok(BoundCheck::Violated {
                point: mid,
                value: gv,
            });
        }
        // bisect widest dimension (relative width)
        let (name, iv) = bx
            .iter()
            .max_by(|a, b| {
                let wa = a.1.width() / a.1.lo.abs().max(a.1.hi.abs()).max(1e-300);
                let wb = b.1.width() / b.1.lo.abs().max(b.1.hi.abs()).max(1e-300);
                wa.partial_cmp(&wb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, v)| (k.clone(), *v))
            .ok_or_else(|| SimorphError::InvalidInput("empty box".into()))?;
        if iv.width() == 0.0 {
            return Ok(BoundCheck::Inconclusive { boxes: processed });
        }
        let m = iv.mid();
        let mut left = bx.clone();
        left.insert(name.clone(), Interval { lo: iv.lo, hi: m });
        let mut right = bx;
        right.insert(name, Interval { lo: m, hi: iv.hi });
        stack.push(left);
        stack.push(right);
    }
    Ok(BoundCheck::Holds {
        boxes: certified,
        rigorous,
        worst_margin: worst,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Expr;

    fn grid_check(text: &str, box_: &[(&str, f64, f64)]) {
        let e = Expr::parse(text).unwrap();
        let b: BTreeMap<String, Interval> = box_
            .iter()
            .map(|(n, l, h)| (n.to_string(), Interval::new(*l, *h)))
            .collect();
        let enc = enclose(&e, &b).unwrap().range;
        let n = 41usize;
        let dims: Vec<&(&str, f64, f64)> = box_.iter().collect();
        let total = n.pow(dims.len() as u32);
        for idx in 0..total {
            let mut env = BTreeMap::new();
            let mut r = idx;
            for (name, l, h) in &dims {
                let k = r % n;
                r /= n;
                env.insert(name.to_string(), l + (h - l) * k as f64 / (n - 1) as f64);
            }
            let v = e.eval(&env).unwrap();
            assert!(
                enc.contains(v),
                "{text}: {v} not in [{}, {}] at {env:?}",
                enc.lo,
                enc.hi
            );
        }
    }

    #[test]
    fn enclosures_contain_dense_samples() {
        grid_check("x^2 - 2*x*y + y^3", &[("x", -2.0, 1.5), ("y", -1.0, 2.0)]);
        grid_check(
            "P*L^3/(3*E*I)",
            &[
                ("P", 0.0, 1000.0),
                ("L", 1.0, 2.0),
                ("E", 2e11, 2.2e11),
                ("I", 1e-5, 2e-5),
            ],
        );
        grid_check(
            "sin(x)*cos(y) + exp(-x^2)",
            &[("x", -3.0, 3.0), ("y", 0.0, 7.0)],
        );
        grid_check(
            "sqrt(x) + ln(x+1) + abs(y - 0.3)",
            &[("x", 0.0, 4.0), ("y", -1.0, 1.0)],
        );
        grid_check("x^(3/2)/(1 + y^2)", &[("x", 0.0, 9.0), ("y", -2.0, 2.0)]);
    }

    #[test]
    fn exactness_of_simple_cases_and_rigour_flag() {
        let e = Expr::parse("x + y").unwrap();
        let b = BTreeMap::from([
            ("x".to_string(), Interval::new(1.0, 2.0)),
            ("y".to_string(), Interval::new(3.0, 4.0)),
        ]);
        let r = enclose(&e, &b).unwrap();
        assert!(
            r.rigorous
                && r.range.lo <= 4.0
                && r.range.hi >= 6.0
                && r.range.lo > 3.999_999_999
                && r.range.hi < 6.000_000_001
        );
        assert!(
            !enclose(&Expr::parse("exp(x)").unwrap(), &b)
                .unwrap()
                .rigorous
        );
        let sq = enclose(
            &Expr::parse("x^2").unwrap(),
            &BTreeMap::from([("x".to_string(), Interval::new(-1.0, 2.0))]),
        )
        .unwrap()
        .range;
        assert_eq!(sq.lo, 0.0);
    }

    #[test]
    fn branch_and_bound_certifies_and_refutes() {
        // x^2 - x + 0.3 > 0 for all x in [-1, 2] (min 0.05 at x=0.5): needs subdivision
        let g = Expr::parse("x^2 - x + 0.3").unwrap();
        let b = BTreeMap::from([("x".to_string(), Interval::new(-1.0, 2.0))]);
        match check_atom_on_box(&g, Cmp::Gt, &b, 10_000).unwrap() {
            BoundCheck::Holds {
                boxes,
                rigorous,
                worst_margin,
            } => {
                assert!(rigorous && boxes > 1 && worst_margin > 0.0);
            }
            other => panic!("{other:?}"),
        }
        let g2 = Expr::parse("x^2 - x + 0.2").unwrap();
        // min is -0.05 at 0.5 → violated
        assert!(matches!(
            check_atom_on_box(&g2, Cmp::Gt, &b, 10_000).unwrap(),
            BoundCheck::Violated { .. }
        ));
    }
}
