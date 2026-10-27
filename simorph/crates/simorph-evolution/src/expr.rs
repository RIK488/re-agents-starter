//! Typed expression trees for genetic programming.
//!
//! Operators are *protected* (Koza-style) so that every tree evaluates to a
//! finite number on finite inputs: `÷` returns 1 when the denominator is
//! below 1e-300 in magnitude, `√` and `ln` act on `|x|`, `exp` saturates its
//! argument at 700.  Non-finite results (overflow) make an individual
//! invalid; they are never silently replaced.
//!
//! Each expression can also be given a physical [`Dimension`]: variables
//! carry the dimension of their unit, constants are dimensionless, sums
//! require identical dimensions and transcendental functions require
//! dimensionless arguments.  [`Expr::dimension`] returns `None` for an
//! internally inconsistent expression (e.g. `m + s`).

use rand::Rng;
use serde::{Deserialize, Serialize};
use simorph_units::Dimension;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnOp {
    Neg,
    Sqrt,
    Exp,
    Ln,
    Sin,
    Cos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Const(f64),
    Var(usize),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Integer power, exponent in `[-3, 3] \ {0, 1}`.
    Pow(Box<Expr>, i32),
}

/// The primitive set available to the search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimitiveSet {
    pub n_vars: usize,
    pub unary: Vec<UnOp>,
    pub binary: Vec<BinOp>,
    pub powers: Vec<i32>,
    /// Range of ephemeral random constants.
    pub const_range: (f64, f64),
    /// Probability that a terminal is a constant (vs a variable).
    pub const_prob: f64,
}

impl PrimitiveSet {
    pub fn arithmetic(n_vars: usize) -> Self {
        Self {
            n_vars,
            unary: vec![UnOp::Sqrt, UnOp::Neg],
            binary: vec![BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div],
            powers: vec![2, 3, -1],
            const_range: (-2.0, 2.0),
            const_prob: 0.3,
        }
    }
    pub fn full(n_vars: usize) -> Self {
        Self {
            n_vars,
            unary: vec![
                UnOp::Sqrt,
                UnOp::Neg,
                UnOp::Exp,
                UnOp::Ln,
                UnOp::Sin,
                UnOp::Cos,
            ],
            binary: vec![BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div],
            powers: vec![2, 3, -1],
            const_range: (-2.0, 2.0),
            const_prob: 0.3,
        }
    }
}

impl Expr {
    pub fn eval(&self, x: &[f64]) -> f64 {
        match self {
            Expr::Const(c) => *c,
            Expr::Var(i) => x[*i],
            Expr::Unary(op, a) => {
                let v = a.eval(x);
                match op {
                    UnOp::Neg => -v,
                    UnOp::Sqrt => v.abs().sqrt(),
                    UnOp::Exp => v.min(700.0).exp(),
                    UnOp::Ln => {
                        let a = v.abs();
                        if a < 1e-300 {
                            0.0
                        } else {
                            a.ln()
                        }
                    }
                    UnOp::Sin => v.sin(),
                    UnOp::Cos => v.cos(),
                }
            }
            Expr::Binary(op, a, b) => {
                let (u, v) = (a.eval(x), b.eval(x));
                match op {
                    BinOp::Add => u + v,
                    BinOp::Sub => u - v,
                    BinOp::Mul => u * v,
                    BinOp::Div => {
                        if v.abs() < 1e-300 {
                            1.0
                        } else {
                            u / v
                        }
                    }
                }
            }
            Expr::Pow(a, n) => {
                let v = a.eval(x);
                if *n < 0 && v.abs() < 1e-300 {
                    1.0
                } else {
                    v.powi(*n)
                }
            }
        }
    }

    /// Number of nodes.
    pub fn size(&self) -> usize {
        match self {
            Expr::Const(_) | Expr::Var(_) => 1,
            Expr::Unary(_, a) | Expr::Pow(a, _) => 1 + a.size(),
            Expr::Binary(_, a, b) => 1 + a.size() + b.size(),
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Expr::Const(_) | Expr::Var(_) => 1,
            Expr::Unary(_, a) | Expr::Pow(a, _) => 1 + a.depth(),
            Expr::Binary(_, a, b) => 1 + a.depth().max(b.depth()),
        }
    }

    /// Bitmask of the variables used (variables 0..63).
    pub fn var_mask(&self) -> u64 {
        match self {
            Expr::Const(_) => 0,
            Expr::Var(i) => 1u64 << (*i).min(63),
            Expr::Unary(_, a) | Expr::Pow(a, _) => a.var_mask(),
            Expr::Binary(_, a, b) => a.var_mask() | b.var_mask(),
        }
    }

    /// Physical dimension, or `None` if the expression is dimensionally
    /// inconsistent.
    pub fn dimension(&self, vars: &[Dimension]) -> Option<Dimension> {
        match self {
            Expr::Const(_) => Some(Dimension::NONE),
            Expr::Var(i) => vars.get(*i).copied(),
            Expr::Unary(op, a) => {
                let d = a.dimension(vars)?;
                match op {
                    UnOp::Neg => Some(d),
                    UnOp::Sqrt => Some(d.pow(1, 2)),
                    UnOp::Exp | UnOp::Ln | UnOp::Sin | UnOp::Cos => {
                        if d.is_dimensionless() {
                            Some(Dimension::NONE)
                        } else {
                            None
                        }
                    }
                }
            }
            Expr::Binary(op, a, b) => {
                let (da, db) = (a.dimension(vars)?, b.dimension(vars)?);
                match op {
                    BinOp::Add | BinOp::Sub => {
                        if da == db {
                            Some(da)
                        } else {
                            None
                        }
                    }
                    BinOp::Mul => Some(da.mul(db)),
                    BinOp::Div => Some(da.div(db)),
                }
            }
            Expr::Pow(a, n) => Some(a.dimension(vars)?.pow(*n as i16, 1)),
        }
    }

    /// Mutable references to every constant (in prefix order).
    pub fn constants_mut(&mut self) -> Vec<&mut f64> {
        let mut out = vec![];
        fn walk<'a>(e: &'a mut Expr, out: &mut Vec<&'a mut f64>) {
            match e {
                Expr::Const(c) => out.push(c),
                Expr::Var(_) => {}
                Expr::Unary(_, a) | Expr::Pow(a, _) => walk(a, out),
                Expr::Binary(_, a, b) => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        walk(self, &mut out);
        out
    }

    pub fn constants(&self) -> Vec<f64> {
        let mut c = self.clone();
        c.constants_mut().into_iter().map(|v| *v).collect()
    }

    pub fn set_constants(&mut self, values: &[f64]) {
        for (slot, v) in self.constants_mut().into_iter().zip(values) {
            *slot = *v;
        }
    }

    /// Number of nodes (for indexing subtrees in prefix order).
    pub fn subtree(&self, idx: usize) -> &Expr {
        fn go<'a>(e: &'a Expr, idx: &mut usize) -> Option<&'a Expr> {
            if *idx == 0 {
                return Some(e);
            }
            *idx -= 1;
            match e {
                Expr::Const(_) | Expr::Var(_) => None,
                Expr::Unary(_, a) | Expr::Pow(a, _) => go(a, idx),
                Expr::Binary(_, a, b) => go(a, idx).or_else(|| go(b, idx)),
            }
        }
        let mut i = idx;
        go(self, &mut i).expect("index within size")
    }

    /// Replace subtree `idx` (prefix order) by `new`.
    pub fn replace_subtree(&mut self, idx: usize, new: Expr) {
        fn go(e: &mut Expr, idx: &mut usize, new: &mut Option<Expr>) -> bool {
            if *idx == 0 {
                *e = new.take().expect("once");
                return true;
            }
            *idx -= 1;
            match e {
                Expr::Const(_) | Expr::Var(_) => false,
                Expr::Unary(_, a) | Expr::Pow(a, _) => go(a, idx, new),
                Expr::Binary(_, a, b) => go(a, idx, new) || go(b, idx, new),
            }
        }
        let mut i = idx;
        let mut n = Some(new);
        go(self, &mut i, &mut n);
    }

    /// Constant folding and trivial identities (for display and parsimony).
    pub fn simplify(&self) -> Expr {
        match self {
            Expr::Const(_) | Expr::Var(_) => self.clone(),
            Expr::Unary(op, a) => {
                let a = a.simplify();
                if let Expr::Const(c) = a {
                    return Expr::Const(Expr::Unary(*op, Box::new(Expr::Const(c))).eval(&[]));
                }
                if let (UnOp::Neg, Expr::Unary(UnOp::Neg, inner)) = (op, &a) {
                    return (**inner).clone();
                }
                Expr::Unary(*op, Box::new(a))
            }
            Expr::Pow(a, n) => {
                let a = a.simplify();
                if let Expr::Const(c) = a {
                    return Expr::Const(Expr::Pow(Box::new(Expr::Const(c)), *n).eval(&[]));
                }
                Expr::Pow(Box::new(a), *n)
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (a.simplify(), b.simplify());
                if let (Expr::Const(x), Expr::Const(y)) = (&a, &b) {
                    return Expr::Const(
                        Expr::Binary(*op, Box::new(Expr::Const(*x)), Box::new(Expr::Const(*y)))
                            .eval(&[]),
                    );
                }
                match (op, &a, &b) {
                    (BinOp::Mul, Expr::Const(c), e) | (BinOp::Mul, e, Expr::Const(c))
                        if *c == 1.0 =>
                    {
                        (*e).clone()
                    }
                    (BinOp::Add, Expr::Const(c), e) | (BinOp::Add, e, Expr::Const(c))
                        if *c == 0.0 =>
                    {
                        (*e).clone()
                    }
                    (BinOp::Div, e, Expr::Const(c)) if *c == 1.0 => (*e).clone(),
                    (BinOp::Sub, e, Expr::Const(c)) if *c == 0.0 => (*e).clone(),
                    _ => Expr::Binary(*op, Box::new(a), Box::new(b)),
                }
            }
        }
    }

    /// Human-readable infix form with variable names.
    pub fn to_string_with(&self, names: &[String]) -> String {
        match self {
            Expr::Const(c) => format!("{c:.6}"),
            Expr::Var(i) => names.get(*i).cloned().unwrap_or_else(|| format!("x{i}")),
            Expr::Unary(op, a) => {
                let s = a.to_string_with(names);
                match op {
                    UnOp::Neg => format!("(-{s})"),
                    UnOp::Sqrt => format!("sqrt({s})"),
                    UnOp::Exp => format!("exp({s})"),
                    UnOp::Ln => format!("ln({s})"),
                    UnOp::Sin => format!("sin({s})"),
                    UnOp::Cos => format!("cos({s})"),
                }
            }
            Expr::Binary(op, a, b) => {
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                };
                format!(
                    "({} {o} {})",
                    a.to_string_with(names),
                    b.to_string_with(names)
                )
            }
            Expr::Pow(a, n) => format!("({})^{n}", a.to_string_with(names)),
        }
    }

    // ------------------------------------------------------------------ random generation

    fn random_terminal<R: Rng>(ps: &PrimitiveSet, rng: &mut R) -> Expr {
        if ps.n_vars == 0 || rng.gen::<f64>() < ps.const_prob {
            Expr::Const(rng.gen_range(ps.const_range.0..ps.const_range.1))
        } else {
            Expr::Var(rng.gen_range(0..ps.n_vars))
        }
    }

    /// `full` (every branch reaches `depth`) or `grow` tree.
    pub fn random<R: Rng>(ps: &PrimitiveSet, depth: usize, full: bool, rng: &mut R) -> Expr {
        if depth <= 1 {
            return Self::random_terminal(ps, rng);
        }
        let n_funcs = ps.unary.len() + ps.binary.len() + ps.powers.len();
        let n_terms = 2;
        if !full && rng.gen_range(0..(n_funcs + n_terms)) < n_terms {
            return Self::random_terminal(ps, rng);
        }
        let k = rng.gen_range(0..n_funcs);
        if k < ps.unary.len() {
            Expr::Unary(
                ps.unary[k],
                Box::new(Self::random(ps, depth - 1, full, rng)),
            )
        } else if k < ps.unary.len() + ps.binary.len() {
            let op = ps.binary[k - ps.unary.len()];
            Expr::Binary(
                op,
                Box::new(Self::random(ps, depth - 1, full, rng)),
                Box::new(Self::random(ps, depth - 1, full, rng)),
            )
        } else {
            let n = ps.powers[k - ps.unary.len() - ps.binary.len()];
            Expr::Pow(Box::new(Self::random(ps, depth - 1, full, rng)), n)
        }
    }

    /// Ramped half-and-half: depths `min..=max`, alternating full/grow.
    pub fn ramped<R: Rng>(
        ps: &PrimitiveSet,
        min_depth: usize,
        max_depth: usize,
        i: usize,
        rng: &mut R,
    ) -> Expr {
        let span = max_depth.saturating_sub(min_depth) + 1;
        let d = min_depth + (i / 2) % span;
        Self::random(ps, d, i.is_multiple_of(2), rng)
    }

    // ------------------------------------------------------------------ variation

    /// Subtree crossover: replace a random subtree of `self` with a random subtree of `other`.
    pub fn crossover<R: Rng>(&self, other: &Expr, rng: &mut R) -> Expr {
        let mut child = self.clone();
        let i = rng.gen_range(0..self.size());
        let j = rng.gen_range(0..other.size());
        child.replace_subtree(i, other.subtree(j).clone());
        child
    }

    /// Subtree mutation: replace a random subtree by a fresh random tree.
    pub fn subtree_mutation<R: Rng>(
        &self,
        ps: &PrimitiveSet,
        max_depth: usize,
        rng: &mut R,
    ) -> Expr {
        let mut child = self.clone();
        let i = rng.gen_range(0..self.size());
        let d = rng.gen_range(1..=max_depth.max(1));
        child.replace_subtree(i, Expr::random(ps, d, false, rng));
        child
    }

    /// Point mutation: change one node's operator/terminal keeping arity.
    pub fn point_mutation<R: Rng>(&self, ps: &PrimitiveSet, rng: &mut R) -> Expr {
        let mut child = self.clone();
        let i = rng.gen_range(0..self.size());
        let node = child.subtree(i).clone();
        let new = match node {
            Expr::Const(_) | Expr::Var(_) => Self::random_terminal(ps, rng),
            Expr::Unary(_, a) if !ps.unary.is_empty() => {
                Expr::Unary(ps.unary[rng.gen_range(0..ps.unary.len())], a)
            }
            Expr::Binary(_, a, b) if !ps.binary.is_empty() => {
                Expr::Binary(ps.binary[rng.gen_range(0..ps.binary.len())], a, b)
            }
            Expr::Pow(a, _) if !ps.powers.is_empty() => {
                Expr::Pow(a, ps.powers[rng.gen_range(0..ps.powers.len())])
            }
            other => other,
        };
        child.replace_subtree(i, new);
        child
    }

    /// Gaussian perturbation of one constant (relative scale `sigma`).
    pub fn constant_mutation<R: Rng>(&self, sigma: f64, rng: &mut R) -> Expr {
        let mut child = self.clone();
        let mut consts = child.constants_mut();
        if consts.is_empty() {
            return child;
        }
        let k = rng.gen_range(0..consts.len());
        let c = &mut consts[k];
        let n: f64 = rand_distr::Distribution::sample(&rand_distr::StandardNormal, rng);
        **c += sigma * n * c.abs().max(0.1);
        child
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_with(&[]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use simorph_units::parse_unit;

    fn kepler() -> Expr {
        // 2π sqrt(a^3 / mu)
        Expr::Binary(
            BinOp::Mul,
            Box::new(Expr::Const(2.0 * std::f64::consts::PI)),
            Box::new(Expr::Unary(
                UnOp::Sqrt,
                Box::new(Expr::Binary(
                    BinOp::Div,
                    Box::new(Expr::Pow(Box::new(Expr::Var(0)), 3)),
                    Box::new(Expr::Var(1)),
                )),
            )),
        )
    }

    #[test]
    fn eval_size_and_dimension() {
        let e = kepler();
        let a = 7.0e6;
        let mu = 3.986e14;
        assert!(
            (e.eval(&[a, mu]) - 2.0 * std::f64::consts::PI * (a * a * a / mu).sqrt()).abs() < 1e-6
        );
        assert_eq!(e.size(), 7);
        let dims = [
            parse_unit("m").unwrap().dimension,
            parse_unit("m^3/s^2").unwrap().dimension,
        ];
        assert_eq!(e.dimension(&dims), Some(parse_unit("s").unwrap().dimension));
        let bad = Expr::Binary(BinOp::Add, Box::new(Expr::Var(0)), Box::new(Expr::Var(1)));
        assert_eq!(bad.dimension(&dims), None);
        let bad2 = Expr::Unary(UnOp::Exp, Box::new(Expr::Var(0)));
        assert_eq!(bad2.dimension(&dims), None);
    }

    #[test]
    fn protected_ops_are_finite_and_variation_preserves_validity() {
        let ps = PrimitiveSet::full(2);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        for i in 0..500 {
            let e = Expr::ramped(&ps, 2, 5, i, &mut rng);
            let v = e.eval(&[0.0, 0.0]);
            assert!(!v.is_nan(), "{e}");
            let c = e.crossover(&Expr::ramped(&ps, 2, 4, i + 1, &mut rng), &mut rng);
            let m = c
                .subtree_mutation(&ps, 3, &mut rng)
                .point_mutation(&ps, &mut rng)
                .constant_mutation(0.1, &mut rng);
            assert!(m.size() >= 1);
        }
    }

    #[test]
    fn constants_round_trip_and_simplify() {
        let mut e = kepler();
        assert_eq!(e.constants().len(), 1);
        e.set_constants(&[1.0]);
        let s = e.simplify();
        assert_eq!(s.size(), 5, "{s}");
        let folded = Expr::Binary(
            BinOp::Add,
            Box::new(Expr::Const(2.0)),
            Box::new(Expr::Const(3.0)),
        )
        .simplify();
        assert_eq!(folded, Expr::Const(5.0));
    }
}
