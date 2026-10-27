//! Scientific-law discovery from simulated experiments.
//!
//! The data are produced by integrating dynamical models with
//! `simorph-simulation` and *measuring* the quantity of interest from the
//! trajectory (event detection); the analytic law is used only afterwards,
//! as an independent reference to judge whether the discovery is correct.

use crate::expr::{Expr, PrimitiveSet};
use crate::gp::*;
use indexmap::IndexMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_plugin_physics::orbit::TwoBody;
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use simorph_simulation::{
    simulate, DynamicalModel, EventAction, EventDirection, Params, Solver, SolverOptions,
};
use simorph_validation::{
    classify, CheckResult, ReferenceOrigin, ValidationRecord, ValidationTarget,
};

/// `(name, reference function, tolerance)` used as an independent check.
pub type Reference<'a> = (&'a str, &'a dyn Fn(&[f64]) -> f64, f64);

// ---------------------------------------------------------------- orbit period measurement

/// The plugins/physics two-body model with one extra event: the rising
/// crossing of the `y = 0` plane, which terminates the integration after
/// exactly one revolution when the orbit starts on the +x axis.
struct OneRevolution;

impl DynamicalModel for OneRevolution {
    fn name(&self) -> &str {
        "evolution.orbit.one_revolution"
    }
    fn state_dim(&self) -> usize {
        TwoBody.state_dim()
    }
    fn state_names(&self) -> Vec<(String, String)> {
        TwoBody.state_names()
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        TwoBody.parameter_names()
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        TwoBody.initial_state(p)
    }
    fn rhs(&self, t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        TwoBody.rhs(t, x, dx, p)
    }
    fn num_events(&self) -> usize {
        1
    }
    fn event_value(&self, _i: usize, _t: f64, x: &[f64], _p: &Params) -> f64 {
        x[1]
    }
    fn event_direction(&self, _i: usize) -> EventDirection {
        EventDirection::Rising
    }
    fn handle_event(&self, _i: usize, _t: f64, _x: &mut [f64], _p: &Params) -> EventAction {
        EventAction::Terminate
    }
}

/// Integrate one revolution of an orbit with periapsis `r_p` and apoapsis
/// `r_a` around a body of gravitational parameter `mu` and return the
/// measured period (s).  The initial velocity follows from the vis-viva
/// equation (this is an initial condition, not the period law).
pub fn measure_orbit_period(mu: f64, r_p: f64, r_a: f64, ctl: &ExecutionControl) -> Result<f64> {
    let a = 0.5 * (r_p + r_a);
    let v_p = (mu * (2.0 / r_p - 1.0 / a)).sqrt();
    let p: Params = [
        ("mu", mu),
        ("r_eq", 1.0),
        ("j2", 0.0),
        ("sun_x", 1.0),
        ("sun_y", 0.0),
        ("sun_z", 0.0),
        ("x0", r_p),
        ("y0", 0.0),
        ("z0", 0.0),
        ("vx0", 0.0),
        ("vy0", v_p),
        ("vz0", 0.0),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect();
    let opts = SolverOptions::new(Solver::Rk45)
        .with_tolerances(1e-11, 1e-6 * r_p.min(1.0))
        .with_record_every(1_000_000);
    let tr = simulate(&OneRevolution, &p, None, (0.0, 1e13), &opts, ctl)?;
    if !tr.terminated_by_event {
        return Err(SimorphError::Numerical(
            "orbit did not complete a revolution".into(),
        ));
    }
    Ok(tr.final_time())
}

/// Central bodies `(name, μ [m³/s²], radius [m])`.
pub fn bodies() -> Vec<(&'static str, f64, f64)> {
    vec![
        ("Earth", 3.986_004_418e14, 6.378_137e6),
        ("Moon", 4.904_869_5e12, 1.737_4e6),
        ("Mars", 4.282_837e13, 3.396_2e6),
        ("Jupiter", 1.266_865_34e17, 7.149_2e7),
        ("Venus", 3.248_59e14, 6.051_8e6),
        ("Sun", 1.327_124_400_18e20, 6.957e8),
    ]
}

/// Kepler data set: variables `a` (m), `mu` (m³/s²), `e` (1, a distractor),
/// target `T` (s) measured by integration, with multiplicative Gaussian
/// timing noise of relative standard deviation `noise_rel`.
pub fn kepler_dataset(
    body_names: &[&str],
    orbits_per_body: usize,
    noise_rel: f64,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<Dataset> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut d = Dataset::new(&["a", "mu", "e"], &["m", "m^3/s^2", "1"], "T", "s");
    for (name, mu, radius) in bodies() {
        if !body_names.contains(&name) {
            continue;
        }
        for _ in 0..orbits_per_body {
            let a = radius * rng.gen_range(1.2..30.0);
            let e: f64 = rng.gen_range(0.0..0.6);
            let t = measure_orbit_period(mu, a * (1.0 - e), a * (1.0 + e), ctl)?;
            let n: f64 = rand_distr::Distribution::sample(&rand_distr::StandardNormal, &mut rng);
            d.push(vec![a, mu, e], t * (1.0 + noise_rel * n));
        }
    }
    Ok(d)
}

// ---------------------------------------------------------------- pendulum

/// Undamped nonlinear pendulum θ'' = −(g/L) sin θ released from θ0 at rest;
/// the event θ = 0 (falling) occurs after a quarter period.
pub struct Pendulum;

impl DynamicalModel for Pendulum {
    fn name(&self) -> &str {
        "evolution.pendulum"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![
            ("theta".into(), "rad".into()),
            ("omega".into(), "rad/s".into()),
        ]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("L".into(), "m".into()),
            ("g".into(), "m/s^2".into()),
            ("theta0".into(), "rad".into()),
        ]
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        vec![p.get("theta0").copied().unwrap_or(0.1), 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        let l = simorph_simulation::model::param(p, "L")?;
        let g = simorph_simulation::model::param(p, "g")?;
        dx[0] = x[1];
        dx[1] = -(g / l) * x[0].sin();
        Ok(())
    }
    fn num_events(&self) -> usize {
        1
    }
    fn event_value(&self, _i: usize, _t: f64, x: &[f64], _p: &Params) -> f64 {
        x[0]
    }
    fn event_direction(&self, _i: usize) -> EventDirection {
        EventDirection::Falling
    }
    fn handle_event(&self, _i: usize, _t: f64, _x: &mut [f64], _p: &Params) -> EventAction {
        EventAction::Terminate
    }
}

/// Measured period: 4 × time of the first zero crossing.
pub fn measure_pendulum_period(l: f64, g: f64, theta0: f64, ctl: &ExecutionControl) -> Result<f64> {
    let p: Params = [("L", l), ("g", g), ("theta0", theta0)]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    let opts = SolverOptions::new(Solver::Rk45)
        .with_tolerances(1e-12, 1e-14)
        .with_record_every(1_000_000);
    let tr = simulate(&Pendulum, &p, None, (0.0, 1e6), &opts, ctl)?;
    if !tr.terminated_by_event {
        return Err(SimorphError::Numerical(
            "pendulum never crossed θ = 0".into(),
        ));
    }
    Ok(4.0 * tr.final_time())
}

/// Pendulum data: variables `L` (m), `g` (m/s²), `theta0` (1); target `T` (s).
pub fn pendulum_dataset(
    n: usize,
    theta_range: (f64, f64),
    noise_rel: f64,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<Dataset> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut d = Dataset::new(&["L", "g", "theta0"], &["m", "m/s^2", "1"], "T", "s");
    let gs = [9.81, 1.62, 3.71, 24.79, 8.87];
    for i in 0..n {
        let l = rng.gen_range(0.1..10.0);
        let g = gs[i % gs.len()];
        let th = rng.gen_range(theta_range.0..theta_range.1);
        let t = measure_pendulum_period(l, g, th, ctl)?;
        let z: f64 = rand_distr::Distribution::sample(&rand_distr::StandardNormal, &mut rng);
        d.push(vec![l, g, th], t * (1.0 + noise_rel * z));
    }
    Ok(d)
}

// ---------------------------------------------------------------- claims

/// A law discovered by GP and its independent validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    pub verdict: LawVerdict,
    pub claim: Claim,
    pub validation: Option<ValidationRecord>,
    pub gp: GpResult,
    pub provenance: Vec<RecordId>,
}

/// Validate a law (held-out error threshold, dimensional homogeneity and an
/// optional comparison with an independent reference function) and turn it
/// into a claim.  The claim is promoted to `Simulated` only if every check
/// passes; it is never promoted beyond that here.
pub fn validate_law(
    verdict: &LawVerdict,
    held_out: &Dataset,
    held_out_threshold: f64,
    reference: Option<Reference<'_>>,
) -> Result<(Claim, Option<ValidationRecord>)> {
    let LawVerdict::Law {
        expr,
        expression,
        held_out_error,
        ..
    } = verdict
    else {
        return Ok((
            Claim::hypothesis(format!(
                "aucune loi trouvée pour `{}`",
                held_out.target_name
            )),
            None,
        ));
    };
    let mut claim = Claim::hypothesis(format!("{} = {}", held_out.target_name, expression));
    let mut checks = vec![];
    checks.push(
        if *held_out_error < held_out_threshold {
            CheckResult::pass(
                "held_out_error",
                format!("erreur hors échantillon {held_out_error:.3e} < {held_out_threshold:.1e}"),
            )
        } else {
            CheckResult::fail(
                "held_out_error",
                format!("erreur hors échantillon {held_out_error:.3e} ≥ {held_out_threshold:.1e}"),
            )
        }
        .metric("held_out_error", *held_out_error),
    );
    let dims = held_out.var_dimensions()?;
    let target = held_out.target_dimension()?;
    checks.push(match expr.dimension(&dims) {
        Some(d) if d == target => CheckResult::pass(
            "dimensional_homogeneity",
            format!("dimension {d} = cible {target}"),
        ),
        Some(d) => CheckResult::fail(
            "dimensional_homogeneity",
            format!("dimension {d} ≠ cible {target}"),
        ),
        None => CheckResult::fail(
            "dimensional_homogeneity",
            "expression dimensionnellement incohérente",
        ),
    });
    if let Some((name, f, tol)) = reference {
        let worst = held_out
            .x
            .iter()
            .map(|x| ((expr.eval(x) - f(x)) / f(x)).abs())
            .fold(0.0, f64::max);
        checks.push(
            if worst < tol {
                CheckResult::pass(
                    "independent_reference",
                    format!("écart max {worst:.3e} à la référence {name} < {tol:.1e}"),
                )
            } else {
                CheckResult::fail(
                    "independent_reference",
                    format!("écart max {worst:.3e} à la référence {name} ≥ {tol:.1e}"),
                )
            }
            .metric("max_relative_deviation", worst),
        );
    }
    let pseudo = TaskResult::new(
        &TaskRequest::new(ComponentId::deterministic("simorph-evolution.gp")),
        EvidenceLevel::Simulated,
    );
    let record = classify(
        ValidationTarget::Claim {
            statement: claim.statement.clone(),
        },
        &pseudo,
        checks,
        &[ReferenceOrigin::Numerical],
        None,
        None,
    );
    if record.verdict == EvidenceLevel::Simulated {
        claim.promote(EvidenceLevel::Simulated, record.id)?;
        claim.caveats.push(
            "loi découverte sur données simulées : non confrontée à des mesures, non démontrée"
                .into(),
        );
    }
    Ok((claim, Some(record)))
}

fn record_run(
    ledger: &Ledger,
    title: &str,
    data: &Dataset,
    gp: &GpResult,
    verdict: &LawVerdict,
    claim: &Claim,
    validation: &Option<ValidationRecord>,
) -> Result<Vec<RecordId>> {
    let mut ids = vec![];
    let ds = ledger.record(ProvenanceKind::Dataset, format!("{title} — données ({} points, hash {})", data.len(), &data.content_hash()[..12]), &serde_json::json!({"hash": data.content_hash(), "names": data.names, "units": data.units, "n": data.len()}), &[])?;
    ids.push(ds);
    let exp = ledger.record(ProvenanceKind::Experiment, format!("{title} — programmation génétique"), &serde_json::json!({"generations": gp.generations, "evaluations": gp.evaluations, "stop": gp.stop_reason}), &[ds])?;
    ids.push(exp);
    for g in &gp.history {
        ids.push(ledger.record(
            ProvenanceKind::Run,
            format!("{title} — génération {}", g.generation),
            g,
            &[exp],
        )?);
    }
    let best = ledger.record(ProvenanceKind::Model, format!("{title} — meilleur individu"), &serde_json::json!({"model": gp.best.model.simplify().to_string_with(&data.names), "error": gp.best.error, "size": gp.best.size}), &[exp])?;
    ids.push(best);
    let mut parents = vec![best];
    if let Some(v) = validation {
        let vid = ledger.record(
            ProvenanceKind::Validation,
            format!("{title} — validation"),
            v,
            &[best],
        )?;
        ids.push(vid);
        parents.push(vid);
    }
    ids.push(ledger.record(
        ProvenanceKind::Decision,
        format!("{title} — verdict"),
        &serde_json::json!({"verdict": verdict, "claim": claim}),
        &parents,
    )?);
    Ok(ids)
}

/// Full pipeline: split, GP, model selection, validation, provenance.
#[allow(clippy::too_many_arguments)]
pub fn discover(
    title: &str,
    data: &Dataset,
    held_out: &Dataset,
    cfg: &GpConfig,
    max_ratio: f64,
    held_out_threshold: f64,
    reference: Option<Reference<'_>>,
    ledger: &Ledger,
    ctl: &ExecutionControl,
) -> Result<Discovery> {
    let gp = run_gp(data, cfg, ctl)?;
    let verdict = select_law(&gp, held_out, cfg.metric, max_ratio);
    let (claim, validation) = validate_law(&verdict, held_out, held_out_threshold, reference)?;
    let provenance = record_run(ledger, title, data, &gp, &verdict, &claim, &validation)?;
    Ok(Discovery {
        verdict,
        claim,
        validation,
        gp,
        provenance,
    })
}

/// Coefficient `c` of `f(θ) ≈ f(0)(1 + c θ² + …)` estimated from a
/// discovered expression by a central second difference at θ = 0.
pub fn quadratic_coefficient(expr: &Expr, var: usize, n_vars: usize, h: f64) -> f64 {
    let at = |t: f64| {
        let mut x = vec![0.0; n_vars];
        x[var] = t;
        expr.eval(&x)
    };
    let f0 = at(0.0);
    (at(h) + at(-h) - 2.0 * f0) / (2.0 * h * h) / f0
}

/// Default primitive set and configuration for physical-law discovery.
pub fn physics_config(n_vars: usize, mode: DimensionMode, seed: u64) -> GpConfig {
    let mut cfg = GpConfig::new(PrimitiveSet::arithmetic(n_vars), seed);
    cfg.dimension_mode = mode;
    cfg.max_size = 20;
    cfg.generations = 30;
    cfg.target_error = 2e-4;
    cfg
}

/// Summary row of a dimension-aware vs plain GP comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeComparison {
    pub mode: String,
    pub runs: usize,
    pub successes: usize,
    pub mean_evaluations_to_stop: f64,
    pub median_held_out_error: f64,
}

/// Run GP `seeds` times per mode on the same data and count successes
/// (held-out relative error below `success_error` *and* correct dimension).
pub fn compare_modes(
    train: &Dataset,
    held_out: &Dataset,
    modes: &[(String, DimensionMode)],
    seeds: u64,
    base: &GpConfig,
    success_error: f64,
    ctl: &ExecutionControl,
) -> Result<Vec<ModeComparison>> {
    let dims = held_out.var_dimensions()?;
    let target = held_out.target_dimension()?;
    let mut out = vec![];
    for (name, mode) in modes {
        let mut errs = vec![];
        let mut evals = vec![];
        let mut ok = 0;
        for s in 0..seeds {
            let mut cfg = base.clone();
            cfg.dimension_mode = *mode;
            cfg.seed = 1000 + s;
            let r = run_gp(train, &cfg, ctl)?;
            let pred: Vec<f64> = held_out.x.iter().map(|x| r.best.model.eval(x)).collect();
            let e = error(cfg.metric, &pred, &held_out.y);
            if e < success_error && r.best.model.dimension(&dims) == Some(target) {
                ok += 1;
            }
            errs.push(e);
            evals.push(r.evaluations as f64);
        }
        errs.sort_by(|a, b| a.total_cmp(b));
        out.push(ModeComparison {
            mode: name.clone(),
            runs: seeds as usize,
            successes: ok,
            mean_evaluations_to_stop: evals.iter().sum::<f64>() / evals.len().max(1) as f64,
            median_held_out_error: errs[errs.len() / 2],
        });
    }
    Ok(out)
}

/// Pure-noise data for the control experiment.
pub fn noise_dataset(n: usize, seed: u64) -> Dataset {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut d = Dataset::new(&["x1", "x2"], &["1", "1"], "y", "1");
    for _ in 0..n {
        let z: f64 = rand_distr::Distribution::sample(&rand_distr::StandardNormal, &mut rng);
        d.push(vec![rng.gen_range(0.0..1.0), rng.gen_range(0.0..1.0)], z);
    }
    d
}

/// Helper used by reports.
pub fn summary(d: &Discovery) -> IndexMap<String, serde_json::Value> {
    let mut m = IndexMap::new();
    m.insert("claim".into(), serde_json::json!(d.claim.statement));
    m.insert("level".into(), serde_json::json!(d.claim.level));
    m.insert("generations".into(), serde_json::json!(d.gp.generations));
    m.insert("evaluations".into(), serde_json::json!(d.gp.evaluations));
    m.insert("wall_time_s".into(), serde_json::json!(d.gp.wall_time_s));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_orbit_period_matches_kepler_and_pendulum_matches_series() {
        let ctl = ExecutionControl::default();
        let mu = 3.986_004_418e14;
        let (rp, ra) = (7.0e6, 1.2e7);
        let t = measure_orbit_period(mu, rp, ra, &ctl).unwrap();
        let a: f64 = 0.5 * (rp + ra);
        let exact = 2.0 * std::f64::consts::PI * (a.powi(3) / mu).sqrt();
        assert!(((t - exact) / exact).abs() < 1e-7, "{t} vs {exact}");
        let (l, g, th) = (1.0, 9.81, 0.5f64);
        let tp = measure_pendulum_period(l, g, th, &ctl).unwrap();
        let t0 = 2.0 * std::f64::consts::PI * (l / g).sqrt();
        let series = t0
            * (1.0 + th * th / 16.0 + 11.0 * th.powi(4) / 3072.0 + 173.0 * th.powi(6) / 737_280.0);
        assert!(((tp - series) / series).abs() < 1e-5, "{tp} vs {series}");
    }
}
