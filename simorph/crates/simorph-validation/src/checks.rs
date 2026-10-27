//! Individual validation checks.  Each returns a [`CheckResult`]; none
//! panics on bad input (errors are reported as failed checks with details).

use crate::expr::{evaluate, referenced_variables, Value};
use crate::record::CheckResult;
use indexmap::IndexMap;
use simorph_core::{
    run_component, Component, ComponentManifest, ExecutionControl, ProblemSpec, ReferenceCase,
    TaskRequest, TaskResult,
};

// ---------------------------------------------------------------------------
// (a) dimensional consistency
// ---------------------------------------------------------------------------

/// Objective and constraint units of a problem must match the output port
/// units of the manifest that is supposed to produce them.
pub fn check_dimensional_against_problem(m: &ComponentManifest, p: &ProblemSpec) -> CheckResult {
    let mut problems = vec![];
    let mut checked = 0;
    let port_unit = |name: &str| {
        m.output_schema
            .iter()
            .find(|q| q.name == name)
            .map(|q| q.unit.clone())
    };
    for o in &p.objectives {
        match port_unit(&o.variable) {
            None => problems.push(format!(
                "objective `{}` reads `{}` which `{}` does not output",
                o.name, o.variable, m.name
            )),
            Some(u) => {
                checked += 1;
                if let Err(e) = simorph_units::check_same_dimension(&o.unit, &u) {
                    problems.push(format!("objective `{}`: {e}", o.name));
                }
            }
        }
    }
    for c in &p.constraints {
        match port_unit(&c.variable) {
            None => problems.push(format!(
                "constraint `{}` reads `{}` which `{}` does not output",
                c.name, c.variable, m.name
            )),
            Some(u) => {
                checked += 1;
                if let Err(e) = simorph_units::check_same_dimension(&c.unit, &u) {
                    problems.push(format!("constraint `{}`: {e}", c.name));
                }
            }
        }
    }
    for port in m.input_schema.iter().chain(m.output_schema.iter()) {
        if let Err(e) = simorph_units::parse_unit(&port.unit) {
            problems.push(format!("port `{}`: {e}", port.name));
        }
    }
    let r = if problems.is_empty() {
        CheckResult::pass(
            "dimensional_consistency",
            format!(
                "{checked} objective/constraint units match `{}` ports",
                m.name
            ),
        )
    } else {
        CheckResult::fail("dimensional_consistency", problems.join("; "))
    };
    r.metric("n_checked", checked as f64)
        .metric("n_problems", problems.len() as f64)
}

/// Check an equation `lhs = rhs` dimensionally, where each symbol is looked
/// up in `units` (symbol → unit expression).  The right-hand side is parsed
/// with a real expression grammar (sums, products, quotients, powers with
/// numeric exponents, unary minus, scientific notation, parentheses and
/// function calls): every term of a sum must share one dimension; `abs` and
/// `min`/`max` preserve dimensions; `sqrt` halves them; transcendental
/// functions (`exp, ln, log10, sin, cos, tan, ...`) require dimensionless
/// arguments and return dimensionless values.
pub fn check_equation_dimensions(equation: &str, units: &IndexMap<String, String>) -> CheckResult {
    let name = "equation_dimensions";
    let Some((lhs, rhs)) = equation.split_once('=') else {
        return CheckResult::fail(name, format!("`{equation}` has no `=`"));
    };
    let lhs = lhs.trim();
    let lhs_dim = match dimension_of(lhs, units) {
        Ok(d) => d,
        Err(e) => return CheckResult::fail(name, format!("left-hand side `{lhs}`: {e}")),
    };
    match dimension_of(rhs, units) {
        Ok(d) if d == lhs_dim => CheckResult::pass(name, format!("`{}` ~ [{lhs_dim}]", rhs.trim())),
        Ok(d) => CheckResult::fail(
            name,
            format!("`{equation}`: left [{lhs_dim}] ≠ right [{d}]"),
        ),
        Err(e) => CheckResult::fail(name, format!("`{equation}`: {e}")),
    }
}

/// Dimension of an arithmetic expression whose symbols have declared units.
pub fn dimension_of(
    expr: &str,
    units: &IndexMap<String, String>,
) -> Result<simorph_units::Dimension, String> {
    let toks = dim_tokens(expr)?;
    let mut p = DimParser {
        t: toks,
        i: 0,
        units,
        depth: 0,
    };
    let d = p.sum()?;
    if p.i != p.t.len() {
        return Err(format!("unexpected trailing input in `{expr}`"));
    }
    Ok(d.0)
}

#[derive(Debug, Clone, PartialEq)]
enum DTok {
    Num(f64),
    Id(String),
    Op(char),
}

fn dim_tokens(s: &str) -> Result<Vec<DTok>, String> {
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
            // scientific notation: e/E followed by optional sign and digits
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '+' || c[j] == '-') {
                    j += 1;
                }
                if j < c.len() && c[j].is_ascii_digit() {
                    while j < c.len() && c[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(DTok::Num(
                t.parse().map_err(|_| format!("bad number `{t}`"))?,
            ));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            out.push(DTok::Id(c[st..i].iter().collect()));
        } else if "+-*/^(),".contains(ch) {
            out.push(DTok::Op(ch));
            i += 1;
        } else {
            return Err(format!("unexpected character `{ch}`"));
        }
    }
    Ok(out)
}

/// (dimension, constant value if the sub-expression is a pure number)
type DimVal = (simorph_units::Dimension, Option<f64>);

struct DimParser<'a> {
    t: Vec<DTok>,
    i: usize,
    units: &'a IndexMap<String, String>,
    depth: usize,
}

impl<'a> DimParser<'a> {
    fn peek_op(&self, op: char) -> bool {
        matches!(self.t.get(self.i), Some(DTok::Op(o)) if *o == op)
    }
    fn sum(&mut self) -> Result<DimVal, String> {
        let mut l = self.prod()?;
        while self.peek_op('+') || self.peek_op('-') {
            let op = if self.peek_op('+') { '+' } else { '-' };
            self.i += 1;
            let r = self.prod()?;
            if r.0 != l.0 {
                return Err(format!("sum of [{}] and [{}]", l.0, r.0));
            }
            l.1 = match (l.1, r.1) {
                (Some(a), Some(b)) => Some(if op == '+' { a + b } else { a - b }),
                _ => None,
            };
        }
        Ok(l)
    }
    fn prod(&mut self) -> Result<DimVal, String> {
        let mut l = self.unary()?;
        while self.peek_op('*') || self.peek_op('/') {
            let mul = self.peek_op('*');
            self.i += 1;
            let r = self.unary()?;
            l = if mul {
                (l.0.mul(r.0), l.1.zip(r.1).map(|(a, b)| a * b))
            } else {
                (l.0.div(r.0), l.1.zip(r.1).map(|(a, b)| a / b))
            };
        }
        Ok(l)
    }
    fn unary(&mut self) -> Result<DimVal, String> {
        self.depth += 1;
        if self.depth > 256 {
            return Err("expression nested too deeply".into());
        }
        let r = if self.peek_op('-') || self.peek_op('+') {
            let neg = self.peek_op('-');
            self.i += 1;
            self.unary()
                .map(|(d, v)| (d, v.map(|x| if neg { -x } else { x })))
        } else {
            self.pow()
        };
        self.depth -= 1;
        r
    }
    fn pow(&mut self) -> Result<DimVal, String> {
        let base = self.atom()?;
        if self.peek_op('^') {
            self.i += 1;
            let e = self.unary()?;
            if !e.0.is_dimensionless() {
                return Err(format!("exponent has dimension [{}]", e.0));
            }
            let Some(x) = e.1 else {
                if base.0.is_dimensionless() {
                    return Ok((base.0, None));
                }
                return Err("non-constant exponent on a dimensional base".into());
            };
            let (p, q) =
                rational(x).ok_or_else(|| format!("exponent {x} is not a simple rational"))?;
            return Ok((base.0.pow(p, q), base.1.map(|b| b.powf(x))));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<DimVal, String> {
        match self.t.get(self.i).cloned() {
            Some(DTok::Num(x)) => {
                self.i += 1;
                Ok((simorph_units::Dimension::NONE, Some(x)))
            }
            Some(DTok::Op('(')) => {
                self.i += 1;
                let v = self.sum()?;
                if !self.peek_op(')') {
                    return Err("expected `)`".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(DTok::Id(name)) => {
                self.i += 1;
                if self.peek_op('(') {
                    self.i += 1;
                    let mut args = vec![self.sum()?];
                    while self.peek_op(',') {
                        self.i += 1;
                        args.push(self.sum()?);
                    }
                    if !self.peek_op(')') {
                        return Err(format!("expected `)` after arguments of `{name}`"));
                    }
                    self.i += 1;
                    return func_dim(&name, &args);
                }
                match self.units.get(&name) {
                    Some(u) => simorph_units::parse_unit(u)
                        .map(|u| (u.dimension, None))
                        .map_err(|e| format!("unit of `{name}`: {e}")),
                    None if name == "pi" || name == "e" => {
                        Ok((simorph_units::Dimension::NONE, None))
                    }
                    None => Err(format!("no unit declared for `{name}`")),
                }
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }
}

fn rational(x: f64) -> Option<(i16, i16)> {
    for q in 1..=12i16 {
        let p = x * q as f64;
        if (p - p.round()).abs() < 1e-9 && p.abs() < 10_000.0 {
            return Some((p.round() as i16, q));
        }
    }
    None
}

fn func_dim(name: &str, args: &[DimVal]) -> Result<DimVal, String> {
    let none = simorph_units::Dimension::NONE;
    match name {
        "abs" if args.len() == 1 => Ok((args[0].0, None)),
        "sqrt" if args.len() == 1 => Ok((args[0].0.pow(1, 2), None)),
        "min" | "max" if !args.is_empty() => {
            if args.iter().any(|a| a.0 != args[0].0) {
                return Err(format!("`{name}` of arguments with different dimensions"));
            }
            Ok((args[0].0, None))
        }
        "exp" | "ln" | "log" | "log10" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
        | "sinh" | "cosh" | "tanh" => {
            if args.len() != 1 {
                return Err(format!("`{name}` takes one argument"));
            }
            if !args[0].0.is_dimensionless() {
                return Err(format!(
                    "`{name}` of a dimensional argument [{}]",
                    args[0].0
                ));
            }
            Ok((none, None))
        }
        _ => Err(format!(
            "function `{name}` is not supported by the dimensional check"
        )),
    }
}

// ---------------------------------------------------------------------------
// (b) invariants
// ---------------------------------------------------------------------------

/// Evaluate every invariant expression against the outputs and metrics of a
/// result.  Non-numeric outputs are ignored; array outputs expose
/// `name.min`, `name.max`, `name.first`, `name.last`.
pub fn check_invariants(invariants: &[String], result: &TaskResult) -> CheckResult {
    let vars = result_variables(result);
    let mut failed = vec![];
    let mut errors = vec![];
    for inv in invariants {
        // An invariant reading a non-finite value cannot be said to hold.
        if let Ok(names) = referenced_variables(inv) {
            let bad: Vec<&String> = names
                .iter()
                .filter(|n| vars.get(*n).map(|v| v.is_nan()).unwrap_or(false))
                .collect();
            if !bad.is_empty() {
                failed.push(format!(
                    "{inv} (non-finite input: {})",
                    bad.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                continue;
            }
        }
        match evaluate(inv, &vars) {
            Ok(Value::Bool(true)) => {}
            Ok(Value::Bool(false)) => failed.push(inv.clone()),
            Ok(Value::Num(x)) => failed.push(format!("{inv} (numeric {x}, expected boolean)")),
            Err(e) => errors.push(format!("{inv}: {e}")),
        }
    }
    let n = invariants.len();
    let r = if failed.is_empty() && errors.is_empty() {
        CheckResult::pass("invariants", format!("{n} invariant(s) hold"))
    } else {
        let mut d = vec![];
        if !failed.is_empty() {
            d.push(format!("violated: {}", failed.join(", ")));
        }
        if !errors.is_empty() {
            d.push(format!("errors: {}", errors.join(", ")));
        }
        CheckResult::fail("invariants", d.join("; "))
    };
    r.metric("n_invariants", n as f64)
        .metric("n_violated", failed.len() as f64)
}

/// Flatten a result into expression variables.  Non-finite numbers are
/// serialised as JSON `null`; they are mapped back to NaN (never dropped) so
/// that invariants reading them fail.  Array outputs expose `name.min`,
/// `name.max`, `name.first`, `name.last` (NaN if any element is NaN or
/// non-numeric) and `name.len` (all elements).
pub fn result_variables(result: &TaskResult) -> IndexMap<String, f64> {
    let mut vars = IndexMap::new();
    let num = |x: &serde_json::Value| -> f64 {
        match x {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
            serde_json::Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => f64::NAN,
        }
    };
    for (k, v) in &result.outputs {
        match v {
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) | serde_json::Value::Null => {
                vars.insert(k.clone(), num(v));
            }
            serde_json::Value::Array(a) => {
                let nums: Vec<f64> = a.iter().map(num).collect();
                vars.insert(format!("{k}.len"), nums.len() as f64);
                if !nums.is_empty() {
                    let any_nan = nums.iter().any(|x| x.is_nan());
                    let fold = |init: f64, f: fn(f64, f64) -> f64| {
                        if any_nan {
                            f64::NAN
                        } else {
                            nums.iter().cloned().fold(init, f)
                        }
                    };
                    vars.insert(format!("{k}.min"), fold(f64::INFINITY, f64::min));
                    vars.insert(format!("{k}.max"), fold(f64::NEG_INFINITY, f64::max));
                    vars.insert(format!("{k}.first"), nums[0]);
                    vars.insert(format!("{k}.last"), nums[nums.len() - 1]);
                }
            }
            _ => {}
        }
    }
    for (k, v) in &result.metrics {
        vars.entry(k.clone()).or_insert(*v);
    }
    vars
}

// ---------------------------------------------------------------------------
// (c) numerical error: Richardson extrapolation / convergence order
// ---------------------------------------------------------------------------

/// Estimated numerical error of a discretised quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct RichardsonEstimate {
    pub observed_order: f64,
    pub extrapolated: f64,
    /// |finest − extrapolated|.
    pub error_estimate: f64,
}

/// Richardson extrapolation from a sequence of (h, value) pairs sorted by
/// decreasing `h`, with a constant refinement ratio.  Needs ≥ 3 points to
/// estimate the order; with exactly 2 points and an `assumed_order` the
/// extrapolation uses that order.
pub fn richardson(
    seq: &[(f64, f64)],
    assumed_order: Option<f64>,
) -> Result<RichardsonEstimate, String> {
    if seq.len() < 2 {
        return Err("need at least two (h, value) points".into());
    }
    let mut s = seq.to_vec();
    s.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    let (h1, f1) = s[n - 1];
    let (h2, f2) = s[n - 2];
    let r = h2 / h1;
    if !(r > 1.0) {
        return Err("refinement ratio must be > 1".into());
    }
    let order = if n >= 3 {
        let (h3, f3) = s[n - 3];
        let r2 = h3 / h2;
        if (r2 - r).abs() > 1e-9 * r {
            return Err("non-constant refinement ratio".into());
        }
        let num = (f3 - f2) / (f2 - f1);
        if !(num > 0.0) || !num.is_finite() {
            return Err("sequence is not monotonically converging".into());
        }
        num.ln() / r.ln()
    } else {
        assumed_order.ok_or("need three points or an assumed order")?
    };
    let extrapolated = f1 + (f1 - f2) / (r.powf(order) - 1.0);
    Ok(RichardsonEstimate {
        observed_order: order,
        extrapolated,
        error_estimate: (f1 - extrapolated).abs(),
    })
}

/// Check that the observed convergence order is at least `expected_order −
/// tolerance` and report the error estimate.
pub fn check_convergence_order(
    seq: &[(f64, f64)],
    expected_order: f64,
    tolerance: f64,
) -> CheckResult {
    // An observed order needs at least three refinement levels; with two
    // points the "observed" order would just be the expected one (audit).
    if seq.len() < 3 {
        return CheckResult::fail(
            "convergence_order",
            format!("insufficient data: {} point(s), at least 3 refinement levels are required to observe an order", seq.len()),
        );
    }
    match richardson(seq, None) {
        Ok(est) => {
            let ok = est.observed_order >= expected_order - tolerance;
            let base = if ok {
                CheckResult::pass(
                    "convergence_order",
                    format!(
                        "observed order {:.3} (expected {expected_order})",
                        est.observed_order
                    ),
                )
            } else {
                CheckResult::fail(
                    "convergence_order",
                    format!(
                        "observed order {:.3} < expected {expected_order} − {tolerance}",
                        est.observed_order
                    ),
                )
            };
            base.metric("observed_order", est.observed_order)
                .metric("extrapolated", est.extrapolated)
                .metric("error_estimate", est.error_estimate)
        }
        Err(e) => CheckResult::fail("convergence_order", e),
    }
}

// ---------------------------------------------------------------------------
// (d) convergence of a residual series
// ---------------------------------------------------------------------------

/// A residual series converges when its final value is below `tol` and the
/// last `window` values are non-increasing on average.
pub fn check_residual_convergence(residuals: &[f64], tol: f64, window: usize) -> CheckResult {
    let name = "residual_convergence";
    let Some(&last) = residuals.last() else {
        return CheckResult::fail(name, "empty residual series");
    };
    if residuals.iter().any(|r| !r.is_finite()) {
        return CheckResult::fail(name, "non-finite residual encountered");
    }
    let w = window.max(2).min(residuals.len());
    let tail = &residuals[residuals.len() - w..];
    let first_half: f64 = tail[..w / 2].iter().sum::<f64>() / (w / 2).max(1) as f64;
    let second_half: f64 = tail[w / 2..].iter().sum::<f64>() / (w - w / 2) as f64;
    let decreasing = second_half <= first_half || w < 2;
    let ok = last <= tol && decreasing;
    let r = if ok {
        CheckResult::pass(name, format!("final residual {last:.3e} ≤ {tol:.1e}"))
    } else if last > tol {
        CheckResult::fail(name, format!("final residual {last:.3e} > {tol:.1e}"))
    } else {
        CheckResult::fail(name, "residual not decreasing over the final window")
    };
    r.metric("final_residual", last)
        .metric("n_iterations", residuals.len() as f64)
}

// ---------------------------------------------------------------------------
// (e) reproducibility
// ---------------------------------------------------------------------------

/// Compare two results of the same request.  Passes when output hashes are
/// equal, or when every numeric leaf agrees within `rtol`/`atol`.
pub fn check_reproducibility(a: &TaskResult, b: &TaskResult, rtol: f64, atol: f64) -> CheckResult {
    let name = "reproducibility";
    if a.request_hash != b.request_hash {
        return CheckResult::fail(name, "results come from different requests");
    }
    if a.output_hash() == b.output_hash() {
        return CheckResult::pass(name, "bit-for-bit identical outputs")
            .metric("max_abs_deviation", 0.0);
    }
    let (mut la, mut lb) = (vec![], vec![]);
    for (k, va) in &a.outputs {
        match b.outputs.get(k) {
            Some(vb) => {
                leaves(va, &mut la);
                leaves(vb, &mut lb);
            }
            None => {
                return CheckResult::fail(name, format!("output `{k}` missing in second result"))
            }
        }
    }
    if la.len() != lb.len() {
        return CheckResult::fail(name, "outputs have different shapes");
    }
    let mut max_dev: f64 = 0.0;
    let mut ok = true;
    for (x, y) in la.iter().zip(lb.iter()) {
        let d = (x - y).abs();
        max_dev = max_dev.max(d);
        if d > atol + rtol * x.abs().max(y.abs()) {
            ok = false;
        }
    }
    let r = if ok {
        CheckResult::pass(
            name,
            format!("outputs agree within rtol={rtol:e}, atol={atol:e} (max |Δ| = {max_dev:.3e})"),
        )
    } else {
        CheckResult::fail(
            name,
            format!("outputs differ: max |Δ| = {max_dev:.3e} exceeds tolerance"),
        )
    };
    r.metric("max_abs_deviation", max_dev)
}

fn leaves(v: &serde_json::Value, out: &mut Vec<f64>) {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                out.push(f)
            }
        }
        serde_json::Value::Bool(b) => out.push(if *b { 1.0 } else { 0.0 }),
        serde_json::Value::Array(a) => a.iter().for_each(|x| leaves(x, out)),
        serde_json::Value::Object(o) => o.values().for_each(|x| leaves(x, out)),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// (f) sensitivity / robustness
// ---------------------------------------------------------------------------

/// Normalised sensitivity of one output to one input.
#[derive(Debug, Clone, PartialEq)]
pub struct Sensitivity {
    pub input: String,
    pub output: String,
    /// (∂y/∂x)·(x/y) — relative change in y per relative change in x.
    pub relative: f64,
    pub absolute: f64,
}

/// Central finite-difference sensitivities of every numeric output to every
/// numeric input in `inputs` (relative step `rel_step`).  Flags any
/// |relative sensitivity| above `threshold` (an output that changes more
/// than `threshold`× as fast as its input is considered fragile).
pub fn check_sensitivity(
    component: &dyn Component,
    base: &TaskRequest,
    inputs: &[&str],
    rel_step: f64,
    threshold: f64,
    ctl: &ExecutionControl,
) -> (CheckResult, Vec<Sensitivity>) {
    let name = "sensitivity";
    let base_res = match run_component(component, base, ctl) {
        Ok(r) => r,
        Err(e) => {
            return (
                CheckResult::fail(name, format!("base run failed: {e}")),
                vec![],
            )
        }
    };
    let mut sens = vec![];
    let mut fragile = vec![];
    let mut max_rel: f64 = 0.0;
    for &inp in inputs {
        let x = match base.inputs.get(inp).and_then(|v| v.as_f64()) {
            Some(x) => x,
            None => {
                return (
                    CheckResult::fail(name, format!("input `{inp}` is not numeric")),
                    vec![],
                )
            }
        };
        let h = if x != 0.0 {
            rel_step * x.abs()
        } else {
            rel_step
        };
        let mut up = base.clone();
        up.inputs.insert(inp.into(), serde_json::Value::from(x + h));
        let mut dn = base.clone();
        dn.inputs.insert(inp.into(), serde_json::Value::from(x - h));
        let (ru, rd) = match (
            run_component(component, &up, ctl),
            run_component(component, &dn, ctl),
        ) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                return (
                    CheckResult::fail(name, format!("perturbed run failed for `{inp}`: {e}")),
                    vec![],
                )
            }
        };
        for (k, v) in &base_res.outputs {
            let Some(y) = v.as_f64() else { continue };
            let (Some(yu), Some(yd)) = (
                ru.outputs.get(k).and_then(|v| v.as_f64()),
                rd.outputs.get(k).and_then(|v| v.as_f64()),
            ) else {
                continue;
            };
            let dydx = (yu - yd) / (2.0 * h);
            let relative = if y != 0.0 && x != 0.0 {
                dydx * x / y
            } else {
                dydx
            };
            if relative.abs() > threshold {
                fragile.push(format!("{k}/{inp}={relative:.3}"));
            }
            max_rel = max_rel.max(relative.abs());
            sens.push(Sensitivity {
                input: inp.into(),
                output: k.clone(),
                relative,
                absolute: dydx,
            });
        }
    }
    let r = if fragile.is_empty() {
        CheckResult::pass(
            name,
            format!("max |relative sensitivity| = {max_rel:.3} ≤ {threshold}"),
        )
    } else {
        CheckResult::fail(name, format!("fragile: {}", fragile.join(", ")))
    };
    (r.metric("max_relative_sensitivity", max_rel), sens)
}

// ---------------------------------------------------------------------------
// (g) statistical consistency vs reference data
// ---------------------------------------------------------------------------

/// Standard normal CDF (Abramowitz–Stegun 7.1.26, |err| < 1.5e-7).
pub fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs() / std::f64::consts::SQRT_2);
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erf = 1.0 - poly * (-(x * x) / 2.0).exp();
    0.5 * (1.0 + if x >= 0.0 { erf } else { -erf })
}

/// Regularised lower incomplete gamma P(a, x) by series / continued fraction
/// (Numerical Recipes), used for the chi-square CDF.
fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let ln_gamma = |z: f64| -> f64 {
        // Lanczos approximation
        let g = 7.0;
        let c = [
            0.999_999_999_999_809_9,
            676.520_368_121_885_1,
            -1_259.139_216_722_402_8,
            771.323_428_777_653_1,
            -176.615_029_162_140_6,
            12.507_343_278_686_905,
            -0.138_571_095_265_720_12,
            9.984_369_578_019_572e-6,
            1.505_632_735_149_311_6e-7,
        ];
        let z = z - 1.0;
        let mut x = c[0];
        for (i, ci) in c.iter().enumerate().skip(1) {
            x += ci / (z + i as f64);
        }
        let t = z + g + 0.5;
        0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
    };
    if x < a + 1.0 {
        let mut sum = 1.0 / a;
        let mut del = sum;
        let mut ap = a;
        for _ in 0..500 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * 1e-15 {
                break;
            }
        }
        sum * (-x + a * x.ln() - ln_gamma(a)).exp()
    } else {
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / 1e-300;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..500 {
            let an = -(i as f64) * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < 1e-300 {
                d = 1e-300;
            }
            c = b + an / c;
            if c.abs() < 1e-300 {
                c = 1e-300;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-15 {
                break;
            }
        }
        1.0 - (-x + a * x.ln() - ln_gamma(a)).exp() * h
    }
}

/// Chi-square survival function with `k` degrees of freedom.
pub fn chi2_sf(x: f64, k: f64) -> f64 {
    1.0 - gamma_p(k / 2.0, x / 2.0)
}

/// Z-test of the mean residual (prediction − reference) against zero, given
/// the reference measurement standard deviations.  Passes when the two-sided
/// p-value ≥ `alpha`.
pub fn check_z_test(
    predicted: &[f64],
    reference: &[f64],
    reference_std: &[f64],
    alpha: f64,
) -> CheckResult {
    let name = "z_test_bias";
    if predicted.len() != reference.len()
        || predicted.len() != reference_std.len()
        || predicted.is_empty()
    {
        return CheckResult::fail(name, "inputs must be non-empty and of equal length");
    }
    let n = predicted.len() as f64;
    // Weighted mean residual in units of sigma
    let z_i: Vec<f64> = predicted
        .iter()
        .zip(reference)
        .zip(reference_std)
        .map(|((p, r), s)| (p - r) / s)
        .collect();
    let z = z_i.iter().sum::<f64>() / n.sqrt();
    let p = 2.0 * (1.0 - normal_cdf(z.abs()));
    let r = if p >= alpha {
        CheckResult::pass(
            name,
            format!("z = {z:.3}, p = {p:.3} ≥ α = {alpha}: no significant bias"),
        )
    } else {
        CheckResult::fail(
            name,
            format!("z = {z:.3}, p = {p:.3e} < α = {alpha}: significant bias"),
        )
    };
    r.metric("z", z).metric("p_value", p)
}

/// Chi-square goodness of fit of residuals normalised by reference std.
pub fn check_chi_square(
    predicted: &[f64],
    reference: &[f64],
    reference_std: &[f64],
    alpha: f64,
) -> CheckResult {
    let name = "chi_square_fit";
    if predicted.len() != reference.len()
        || predicted.len() != reference_std.len()
        || predicted.is_empty()
    {
        return CheckResult::fail(name, "inputs must be non-empty and of equal length");
    }
    let chi2: f64 = predicted
        .iter()
        .zip(reference)
        .zip(reference_std)
        .map(|((p, r), s)| ((p - r) / s).powi(2))
        .sum();
    let k = predicted.len() as f64;
    let p = chi2_sf(chi2, k);
    let r = if p >= alpha {
        CheckResult::pass(
            name,
            format!("χ² = {chi2:.3} (k = {k}), p = {p:.3}: consistent with reference scatter"),
        )
    } else {
        CheckResult::fail(name, format!("χ² = {chi2:.3} (k = {k}), p = {p:.3e} < α: residuals larger than reference uncertainty"))
    };
    r.metric("chi2", chi2)
        .metric("reduced_chi2", chi2 / k)
        .metric("p_value", p)
}

/// Coverage check: the fraction of reference values inside
/// `predicted ± z·predicted_std` must be at least `expected_coverage −
/// tolerance` (e.g. z = 1.96, expected 0.95).
pub fn check_coverage(
    predicted: &[f64],
    predicted_std: &[f64],
    reference: &[f64],
    z: f64,
    expected_coverage: f64,
    tolerance: f64,
) -> CheckResult {
    let name = "uncertainty_coverage";
    if predicted.len() != reference.len()
        || predicted.len() != predicted_std.len()
        || predicted.is_empty()
    {
        return CheckResult::fail(name, "inputs must be non-empty and of equal length");
    }
    let inside = predicted
        .iter()
        .zip(predicted_std)
        .zip(reference)
        .filter(|((p, s), r)| (*r - *p).abs() <= z * *s)
        .count();
    let frac = inside as f64 / predicted.len() as f64;
    let r = if frac >= expected_coverage - tolerance {
        CheckResult::pass(
            name,
            format!("{frac:.3} of references inside ±{z}σ (expected ≥ {expected_coverage})"),
        )
    } else {
        CheckResult::fail(name, format!("only {frac:.3} of references inside ±{z}σ (expected {expected_coverage}): declared uncertainty too small"))
    };
    r.metric("coverage", frac)
}

// ---------------------------------------------------------------------------
// (h) reference-case comparison
// ---------------------------------------------------------------------------

/// Run every reference case through the component and compare each expected
/// output with rtol/atol.  Returns one CheckResult per case.
pub fn check_reference_cases(
    component: &dyn Component,
    cases: &[ReferenceCase],
    ctl: &ExecutionControl,
) -> Vec<CheckResult> {
    let mut out = vec![];
    for case in cases {
        let name = format!("reference_case:{}", case.name);
        let mut req = TaskRequest::new(component.manifest().id);
        if let Some(obj) = case.inputs.as_object() {
            for (k, v) in obj {
                req.inputs.insert(k.clone(), v.clone());
            }
        }
        if let Some(seed) = case.inputs.get("__seed").and_then(|v| v.as_u64()) {
            req.seed = Some(seed);
        }
        let res = match run_component(component, &req, ctl) {
            Ok(r) => r,
            Err(e) => {
                out.push(CheckResult::fail(&name, format!("run failed: {e}")));
                continue;
            }
        };
        let mut worst: f64 = 0.0;
        let mut problems = vec![];
        if let Some(exp) = case.expected_outputs.as_object() {
            for (k, ev) in exp {
                let Some(av) = res.outputs.get(k) else {
                    problems.push(format!("missing output `{k}`"));
                    continue;
                };
                let (mut le, mut la) = (vec![], vec![]);
                leaves(ev, &mut le);
                leaves(av, &mut la);
                if le.len() != la.len() {
                    problems.push(format!("output `{k}` shape mismatch"));
                    continue;
                }
                for (e, a) in le.iter().zip(la.iter()) {
                    let d = (a - e).abs();
                    let tol = case.atol + case.rtol * e.abs();
                    let ratio = if tol > 0.0 {
                        d / tol
                    } else if d == 0.0 {
                        0.0
                    } else {
                        f64::INFINITY
                    };
                    worst = worst.max(ratio);
                    if d > tol {
                        problems.push(format!(
                            "`{k}`: got {a:.6e}, expected {e:.6e} (|Δ| = {d:.3e} > {tol:.3e})"
                        ));
                    }
                }
            }
        }
        let r = if problems.is_empty() {
            CheckResult::pass(
                &name,
                format!("matches `{}` (worst |Δ|/tol = {worst:.3})", case.source),
            )
        } else {
            CheckResult::fail(&name, problems.join("; "))
        };
        out.push(r.metric("worst_error_ratio", worst));
    }
    out
}

// ---------------------------------------------------------------------------
// (i) generalisation
// ---------------------------------------------------------------------------

/// For data-driven models: the test metric must not be worse than the train
/// metric by more than `max_gap` (relative) and must be at least
/// `min_acceptable` (for "higher is better" metrics; pass
/// `higher_is_better=false` for losses).
pub fn check_generalisation(
    train_metric: f64,
    test_metric: f64,
    higher_is_better: bool,
    max_relative_gap: f64,
    acceptable: f64,
) -> CheckResult {
    let name = "generalisation";
    let gap = if higher_is_better {
        (train_metric - test_metric) / train_metric.abs().max(1e-12)
    } else {
        (test_metric - train_metric) / train_metric.abs().max(1e-12)
    };
    let acceptable_ok = if higher_is_better {
        test_metric >= acceptable
    } else {
        test_metric <= acceptable
    };
    let r = if gap <= max_relative_gap && acceptable_ok {
        CheckResult::pass(
            name,
            format!(
                "train {train_metric:.4}, test {test_metric:.4}, gap {gap:.3} ≤ {max_relative_gap}"
            ),
        )
    } else if !acceptable_ok {
        CheckResult::fail(
            name,
            format!("test metric {test_metric:.4} does not meet the acceptable level {acceptable}"),
        )
    } else {
        CheckResult::fail(
            name,
            format!("train/test gap {gap:.3} > {max_relative_gap}: overfitting suspected"),
        )
    };
    r.metric("train", train_metric)
        .metric("test", test_metric)
        .metric("relative_gap", gap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::{
        Constraint, ConstraintKind, Direction, EvidenceLevel, Objective, Port, Result,
    };

    struct Lin {
        m: ComponentManifest,
    }
    impl Component for Lin {
        fn manifest(&self) -> &ComponentManifest {
            &self.m
        }
        fn execute(&self, req: &TaskRequest, _: &ExecutionControl) -> Result<TaskResult> {
            let x = req.get_f64("x")?;
            let k = req.get_f64_or("k", 1.0);
            Ok(TaskResult::new(req, EvidenceLevel::Computed)
                .output("y", k * x * x)
                .output("z", 3.0)
                .metric("energy_drift", 1e-9))
        }
    }
    fn lin() -> Lin {
        Lin {
            m: ComponentManifest::in_process("lin", "1", "t")
                .with_inputs(vec![Port::scalar("x", "m"), Port::scalar("k", "1/m")])
                .with_outputs(vec![Port::scalar("y", "m"), Port::scalar("z", "1")]),
        }
    }

    #[test]
    fn dimensional_against_problem() {
        let mut p = ProblemSpec::new("p", "d");
        p.objectives.push(Objective {
            name: "o".into(),
            direction: Direction::Minimize,
            variable: "y".into(),
            unit: "km".into(),
            target: None,
            weight: 1.0,
        });
        assert!(check_dimensional_against_problem(&lin().m, &p).passed);
        p.constraints.push(Constraint {
            name: "c".into(),
            variable: "y".into(),
            kind: ConstraintKind::UpperBound,
            bound: 1.0,
            unit: "s".into(),
            tolerance: 0.0,
            hard: true,
        });
        let r = check_dimensional_against_problem(&lin().m, &p);
        assert!(!r.passed);
        assert!(r.details.contains("constraint `c`"));
    }

    #[test]
    fn equation_dimensions() {
        let units: IndexMap<String, String> = [
            ("P", "W"),
            ("U", "V"),
            ("I", "A"),
            ("R", "ohm"),
            ("E", "J"),
            ("m", "kg"),
            ("c", "m/s"),
            ("t", "s"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert!(check_equation_dimensions("P = U*I", &units).passed);
        assert!(check_equation_dimensions("P = R*I^2 + U*I", &units).passed);
        assert!(check_equation_dimensions("E = m*c^2", &units).passed);
        assert!(check_equation_dimensions("E = 0.5*m*c^2", &units).passed);
        assert!(!check_equation_dimensions("E = m*c", &units).passed);
        assert!(!check_equation_dimensions("P = U*I + t", &units).passed);
        assert!(!check_equation_dimensions("P = U*Q", &units).passed);
    }

    #[test]
    fn invariants_on_result() {
        let c = lin();
        let req = TaskRequest::new(c.m.id).input("x", 2.0).input("k", 1.0);
        let res = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        let ok = check_invariants(
            &[
                "y > 0".into(),
                "abs(energy_drift) < 1e-6".into(),
                "z == 3".into(),
            ],
            &res,
        );
        assert!(ok.passed, "{}", ok.details);
        let bad = check_invariants(&["y < 1".into(), "missing > 0".into()], &res);
        assert!(!bad.passed);
        assert_eq!(bad.metrics["n_violated"], 1.0);
        let arr =
            TaskResult::new(&req, EvidenceLevel::Simulated).output("T", vec![250.0, 300.0, 280.0]);
        assert!(
            check_invariants(&["T.min >= 250 && T.max <= 300 && T.len == 3".into()], &arr).passed
        );
    }

    #[test]
    fn richardson_recovers_second_order() {
        // value(h) = 1 + 0.5 h^2 ; h = 0.4, 0.2, 0.1
        let seq = [
            (0.4, 1.0 + 0.5 * 0.16),
            (0.2, 1.0 + 0.5 * 0.04),
            (0.1, 1.0 + 0.5 * 0.01),
        ];
        let est = richardson(&seq, None).unwrap();
        assert!((est.observed_order - 2.0).abs() < 1e-9);
        assert!((est.extrapolated - 1.0).abs() < 1e-12);
        let r = check_convergence_order(&seq, 2.0, 0.1);
        assert!(r.passed);
        let r1 = check_convergence_order(&seq, 4.0, 0.1);
        assert!(!r1.passed);
        // first-order sequence
        let seq1 = [(0.4, 1.0 + 0.4), (0.2, 1.0 + 0.2), (0.1, 1.0 + 0.1)];
        assert!((richardson(&seq1, None).unwrap().observed_order - 1.0).abs() < 1e-9);
        assert!(richardson(&[(0.1, 1.0)], None).is_err());
    }

    #[test]
    fn residual_convergence() {
        let res: Vec<f64> = (0..20).map(|i| 10f64.powi(-(i as i32))).collect();
        assert!(check_residual_convergence(&res, 1e-10, 5).passed);
        assert!(!check_residual_convergence(&res, 1e-30, 5).passed);
        let stuck = vec![1.0, 0.1, 0.01, 0.01, 0.02, 0.05, 0.001];
        assert!(!check_residual_convergence(&stuck, 1e-2, 4).passed);
        assert!(!check_residual_convergence(&[1.0, f64::NAN], 1.0, 2).passed);
    }

    #[test]
    fn reproducibility() {
        let c = lin();
        let req = TaskRequest::new(c.m.id)
            .input("x", 2.0)
            .input("k", 1.0)
            .seed(1);
        let a = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        let b = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        assert!(check_reproducibility(&a, &b, 0.0, 0.0).passed);
        let mut b2 = b.clone();
        b2.outputs
            .insert("y".into(), serde_json::Value::from(4.0 + 1e-9));
        assert!(check_reproducibility(&a, &b2, 1e-8, 0.0).passed);
        assert!(!check_reproducibility(&a, &b2, 1e-12, 0.0).passed);
        let other = TaskRequest::new(c.m.id).input("x", 3.0).input("k", 1.0);
        let c2 = run_component(&c, &other, &ExecutionControl::default()).unwrap();
        assert!(!check_reproducibility(&a, &c2, 1.0, 1.0).passed);
    }

    #[test]
    fn sensitivity_flags_fragile_output() {
        let c = lin();
        let base = TaskRequest::new(c.m.id).input("x", 2.0).input("k", 1.0);
        let (r, s) = check_sensitivity(
            &c,
            &base,
            &["x", "k"],
            1e-4,
            3.0,
            &ExecutionControl::default(),
        );
        assert!(r.passed, "{}", r.details);
        let sx = s
            .iter()
            .find(|q| q.input == "x" && q.output == "y")
            .unwrap();
        assert!((sx.relative - 2.0).abs() < 1e-6, "{sx:?}"); // y = k x^2 ⇒ elasticity 2
        let sk = s
            .iter()
            .find(|q| q.input == "k" && q.output == "y")
            .unwrap();
        assert!((sk.relative - 1.0).abs() < 1e-6);
        let (r2, _) = check_sensitivity(&c, &base, &["x"], 1e-4, 1.5, &ExecutionControl::default());
        assert!(!r2.passed);
    }

    #[test]
    fn statistics() {
        let reference = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let std = vec![0.1; 8];
        let unbiased: Vec<f64> = reference
            .iter()
            .enumerate()
            .map(|(i, r)| r + if i % 2 == 0 { 0.05 } else { -0.05 })
            .collect();
        assert!(check_z_test(&unbiased, &reference, &std, 0.05).passed);
        assert!(check_chi_square(&unbiased, &reference, &std, 0.05).passed);
        let biased: Vec<f64> = reference.iter().map(|r| r + 0.3).collect();
        let z = check_z_test(&biased, &reference, &std, 0.05);
        assert!(!z.passed, "{}", z.details);
        assert!(!check_chi_square(&biased, &reference, &std, 0.05).passed);
        // coverage: declared std too small
        let pred_std = vec![0.01; 8];
        assert!(!check_coverage(&biased, &pred_std, &reference, 1.96, 0.95, 0.05).passed);
        assert!(check_coverage(&unbiased, &std, &reference, 1.96, 0.95, 0.05).passed);
        // sanity of the distribution helpers
        assert!((normal_cdf(1.96) - 0.975).abs() < 1e-4);
        assert!((chi2_sf(3.841, 1.0) - 0.05).abs() < 1e-3);
        assert!((chi2_sf(18.307, 10.0) - 0.05).abs() < 1e-3);
    }

    #[test]
    fn reference_cases() {
        let c = lin();
        let cases = vec![
            ReferenceCase {
                name: "x2".into(),
                inputs: serde_json::json!({"x": 2.0, "k": 1.0}),
                expected_outputs: serde_json::json!({"y": 4.0}),
                rtol: 1e-9,
                atol: 0.0,
                source: "analytic".into(),
            },
            ReferenceCase {
                name: "wrong".into(),
                inputs: serde_json::json!({"x": 2.0, "k": 1.0}),
                expected_outputs: serde_json::json!({"y": 5.0}),
                rtol: 1e-3,
                atol: 0.0,
                source: "analytic".into(),
            },
        ];
        let rs = check_reference_cases(&c, &cases, &ExecutionControl::default());
        assert!(rs[0].passed);
        assert!(!rs[1].passed);
    }

    #[test]
    fn generalisation() {
        assert!(check_generalisation(0.95, 0.93, true, 0.05, 0.9).passed);
        assert!(!check_generalisation(0.99, 0.80, true, 0.05, 0.7).passed);
        assert!(!check_generalisation(0.95, 0.93, true, 0.05, 0.94).passed);
        assert!(check_generalisation(0.10, 0.11, false, 0.2, 0.2).passed);
    }

    #[test]
    fn audit_equation_dimensions_handle_exponents_scientific_notation_and_functions() {
        let mut u = IndexMap::new();
        for (k, v) in [
            ("x", "m"),
            ("t", "s"),
            ("v", "m/s"),
            ("k", "1/s"),
            ("x0", "m"),
            ("g", "m/s^2"),
            ("L", "m"),
            ("T", "s"),
            ("E", "J"),
            ("m", "kg"),
            ("c", "m/s"),
        ] {
            u.insert(k.to_string(), v.to_string());
        }
        // `t^-1` and `1e-3` used to be split on `-`.
        assert!(check_equation_dimensions("k = t^-1", &u).passed);
        assert!(check_equation_dimensions("x = 1e-3*v*t + 2e3*x0", &u).passed);
        assert!(check_equation_dimensions("x = x0*exp(-k*t)", &u).passed);
        assert!(check_equation_dimensions("T = 2*pi*sqrt(L/g)", &u).passed);
        // Wrong: exp of a dimensional argument, E = m c.
        assert!(!check_equation_dimensions("x = x0*exp(-t)", &u).passed);
        assert!(!check_equation_dimensions("E = m*c", &u).passed);
        assert!(check_equation_dimensions("E = m*c^2", &u).passed);
        assert!(!check_equation_dimensions("x = v + t", &u).passed);
    }

    #[test]
    fn audit_convergence_order_needs_three_points() {
        let r = check_convergence_order(&[(0.1, 1.0), (0.05, 2.0)], 2.0, 0.1);
        assert!(!r.passed && r.details.contains("insufficient"));
    }

    #[test]
    fn audit_invariants_fail_on_nan_inputs_and_arrays_keep_nan() {
        let mut res = TaskResult::new(
            &TaskRequest::new(simorph_core::ComponentId::new()),
            simorph_core::EvidenceLevel::Simulated,
        );
        res.outputs
            .insert("T".into(), serde_json::json!([250.0, null, 280.0]));
        res.outputs.insert("y".into(), serde_json::Value::Null);
        assert!(!check_invariants(&["T.max <= 300 && T.len == 3".into()], &res).passed);
        assert!(!check_invariants(&["y < 1".into()], &res).passed);
        assert!(!check_invariants(&["max(y, 0) < 1e-6".into()], &res).passed);
        let mut ok = TaskResult::new(
            &TaskRequest::new(simorph_core::ComponentId::new()),
            simorph_core::EvidenceLevel::Simulated,
        );
        ok.outputs
            .insert("T".into(), serde_json::json!([250.0, 260.0, 280.0]));
        assert!(check_invariants(&["T.max <= 300 && T.len == 3".into()], &ok).passed);
    }
}
