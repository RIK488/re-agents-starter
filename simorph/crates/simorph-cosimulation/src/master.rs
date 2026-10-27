//! Master algorithms.

use crate::coupling::Coupling;
use crate::slave::find_port;
use indexmap::IndexMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};
use simorph_simulation::Params;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterAlgorithm {
    /// All slaves step in parallel using inputs from the previous macro step.
    Jacobi,
    /// Slaves step sequentially in dependency order; downstream slaves use
    /// the freshly computed outputs of upstream slaves.
    GaussSeidel,
    /// Gauss–Seidel repeated with rollback until the exchanged signals
    /// converge (solves algebraic loops).
    IterativeGaussSeidel { max_iter: usize, tol: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterOptions {
    pub algorithm: MasterAlgorithm,
    pub t0: f64,
    pub t_end: f64,
    /// Communication (macro) step.
    pub macro_step: f64,
    /// Record every n-th macro step.
    pub record_every: usize,
    /// When set, each macro step is also taken as two half steps; the
    /// difference (scaled by `atol + rtol·|y|`) is the error estimate and
    /// steps exceeding 1 are halved and retried.  Requires rollback support.
    pub step_doubling: Option<(f64, f64)>,
    pub min_macro_step: f64,
}

impl MasterOptions {
    pub fn new(algorithm: MasterAlgorithm, t_end: f64, macro_step: f64) -> Self {
        Self {
            algorithm,
            t0: 0.0,
            t_end,
            macro_step,
            record_every: 1,
            step_doubling: None,
            min_macro_step: 1e-9,
        }
    }
    pub fn with_step_doubling(mut self, rtol: f64, atol: f64) -> Self {
        self.step_doubling = Some((rtol, atol));
        self
    }
    pub fn with_record_every(mut self, n: usize) -> Self {
        self.record_every = n.max(1);
        self
    }
}

/// Recorded co-simulation signals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CosimTrajectory {
    pub times: Vec<f64>,
    /// `"slave.port"` -> series.
    pub signals: IndexMap<String, Vec<f64>>,
    pub macro_steps: u64,
    pub rejected_steps: u64,
    pub fixed_point_iterations: u64,
    pub max_error_estimate: f64,
    pub algebraic_loops: Vec<Vec<String>>,
}

impl CosimTrajectory {
    pub fn signal(&self, name: &str) -> Option<&[f64]> {
        self.signals.get(name).map(|v| v.as_slice())
    }
    pub fn final_value(&self, name: &str) -> Option<f64> {
        self.signals.get(name).and_then(|v| v.last().copied())
    }
}

/// Output cache: `cache[slave][k]` is the k-th scalar output of the slave.
type Cache = Vec<Vec<f64>>;

fn read_outputs(c: &Coupling, i: usize, t: f64) -> Result<Vec<f64>> {
    let s = &c.slaves[i];
    let mut v = Vec::with_capacity(s.output_names.len());
    for name in &s.output_names {
        let x = s.slave.get_output(name)?;
        if !x.is_finite() {
            return Err(SimorphError::Numerical(format!(
                "slave `{}` produced non-finite output `{name}` at t={t}",
                s.name
            )));
        }
        if let Some(p) = find_port(&s.slave.manifest().output_schema, name) {
            if let Some((lo, hi)) = p.range {
                if x < lo || x > hi {
                    return Err(SimorphError::ValidityDomain(format!(
                        "slave `{}` output `{name}`={x} outside [{lo}, {hi}] at t={t}",
                        s.name
                    )));
                }
            }
        }
        v.push(x);
    }
    Ok(v)
}

fn output_index(c: &Coupling, slave: usize, port: &str) -> usize {
    c.slaves[slave]
        .output_names
        .iter()
        .position(|n| n == port)
        .expect("connection validated at build")
}

/// Push cached outputs into the inputs of slave `to` (all connections
/// targeting it).
fn feed_inputs(c: &mut Coupling, cache: &Cache, to: usize) -> Result<()> {
    let values: Vec<(String, f64)> = c
        .connections
        .iter()
        .filter(|k| k.to_slave == to)
        .map(|k| {
            (
                k.to_port.clone(),
                k.convert(cache[k.from_slave][output_index(c, k.from_slave, &k.from_port)]),
            )
        })
        .collect();
    for (port, v) in values {
        c.slaves[to].slave.set_input(&port, v)?;
    }
    Ok(())
}

fn step_jacobi(c: &mut Coupling, cache: &mut Cache, t: f64, h: f64) -> Result<()> {
    for i in 0..c.slaves.len() {
        feed_inputs(c, cache, i)?;
    }
    let results: Vec<Result<()>> = c
        .slaves
        .par_iter_mut()
        .map(|s| s.slave.do_step(t, h))
        .collect();
    for (i, r) in results.into_iter().enumerate() {
        r.map_err(|e| SimorphError::Component {
            component: c.slaves[i].name.clone(),
            message: format!("{e} at t={t}"),
        })?;
    }
    for i in 0..c.slaves.len() {
        cache[i] = read_outputs(c, i, t + h)?;
    }
    Ok(())
}

fn step_gauss_seidel(c: &mut Coupling, cache: &mut Cache, t: f64, h: f64) -> Result<()> {
    let order = c.order.clone();
    for i in order {
        feed_inputs(c, cache, i)?;
        c.slaves[i]
            .slave
            .do_step(t, h)
            .map_err(|e| SimorphError::Component {
                component: c.slaves[i].name.clone(),
                message: format!("{e} at t={t}"),
            })?;
        cache[i] = read_outputs(c, i, t + h)?;
    }
    Ok(())
}

fn save_all(c: &Coupling) -> Result<Vec<Vec<f64>>> {
    c.slaves
        .iter()
        .map(|s| {
            if !s.slave.supports_rollback() {
                return Err(SimorphError::Incompatible(format!(
                    "slave `{}` does not support rollback (required by the selected master)",
                    s.name
                )));
            }
            s.slave.save_state()
        })
        .collect()
}

fn restore_all(c: &mut Coupling, snaps: &[Vec<f64>]) -> Result<()> {
    for (s, snap) in c.slaves.iter_mut().zip(snaps) {
        s.slave.restore_state(snap)?;
    }
    Ok(())
}

fn exchanged_diff(c: &Coupling, a: &Cache, b: &Cache) -> f64 {
    let mut m = 0.0f64;
    for k in &c.connections {
        let oi = output_index(c, k.from_slave, &k.from_port);
        let (x, y) = (a[k.from_slave][oi], b[k.from_slave][oi]);
        m = m.max((x - y).abs() / (1.0 + y.abs()));
    }
    m
}

fn step_iterative(
    c: &mut Coupling,
    cache: &mut Cache,
    t: f64,
    h: f64,
    max_iter: usize,
    tol: f64,
    iters: &mut u64,
) -> Result<()> {
    let snaps = save_all(c)?;
    let mut prev = cache.clone();
    for it in 0..max_iter {
        restore_all(c, &snaps)?;
        let mut cur = prev.clone();
        step_gauss_seidel(c, &mut cur, t, h)?;
        *iters += 1;
        let d = exchanged_diff(c, &cur, &prev);
        prev = cur;
        if d <= tol {
            *cache = prev;
            return Ok(());
        }
        if it + 1 == max_iter {
            return Err(SimorphError::Numerical(format!(
                "fixed-point iteration did not converge at t={t} (residual {d:e} > {tol:e} after {max_iter} iterations)"
            )));
        }
    }
    unreachable!()
}

fn one_macro_step(
    c: &mut Coupling,
    cache: &mut Cache,
    t: f64,
    h: f64,
    opts: &MasterOptions,
    iters: &mut u64,
) -> Result<()> {
    match opts.algorithm {
        MasterAlgorithm::Jacobi => step_jacobi(c, cache, t, h),
        MasterAlgorithm::GaussSeidel => step_gauss_seidel(c, cache, t, h),
        MasterAlgorithm::IterativeGaussSeidel { max_iter, tol } => {
            step_iterative(c, cache, t, h, max_iter, tol, iters)
        }
    }
}

/// Run the coupled simulation.
pub fn run(
    c: &mut Coupling,
    opts: &MasterOptions,
    ctl: &ExecutionControl,
) -> Result<CosimTrajectory> {
    if !(opts.macro_step > 0.0) || !(opts.t_end > opts.t0) {
        return Err(SimorphError::InvalidInput(
            "macro_step must be > 0 and t_end > t0".into(),
        ));
    }
    if c.has_algebraic_loops()
        && !matches!(opts.algorithm, MasterAlgorithm::IterativeGaussSeidel { .. })
    {
        return Err(SimorphError::Incompatible(format!(
            "coupling has algebraic loops {:?}; use IterativeGaussSeidel",
            c.algebraic_loops
        )));
    }
    // Initialise slaves and exchange initial outputs.
    for i in 0..c.slaves.len() {
        let p = c.slaves[i].params.clone();
        c.slaves[i]
            .slave
            .initialize(opts.t0, &p)
            .map_err(|e| SimorphError::Component {
                component: c.slaves[i].name.clone(),
                message: e.to_string(),
            })?;
    }
    let mut cache: Cache = (0..c.slaves.len())
        .map(|i| read_outputs(c, i, opts.t0))
        .collect::<Result<_>>()?;
    for i in 0..c.slaves.len() {
        feed_inputs(c, &cache, i)?;
    }
    // Feedthrough slaves see their initial inputs: re-step them by zero.
    for i in c.order.clone() {
        if c.slaves[i].slave.has_direct_feedthrough() {
            feed_inputs(c, &cache, i)?;
            c.slaves[i].slave.do_step(opts.t0, 0.0)?;
            cache[i] = read_outputs(c, i, opts.t0)?;
        }
    }

    let names = c.signal_names();
    let mut traj = CosimTrajectory {
        times: vec![],
        signals: names.iter().map(|n| (n.clone(), vec![])).collect(),
        macro_steps: 0,
        rejected_steps: 0,
        fixed_point_iterations: 0,
        max_error_estimate: 0.0,
        algebraic_loops: c.algebraic_loops.clone(),
    };
    let record = |traj: &mut CosimTrajectory, t: f64, cache: &Cache| {
        traj.times.push(t);
        let mut k = 0;
        for s in cache {
            for v in s {
                traj.signals[k].push(*v);
                k += 1;
            }
        }
    };
    record(&mut traj, opts.t0, &cache);

    let mut t = opts.t0;
    let mut h = opts.macro_step;
    let mut since_record = 0usize;
    let mut iters = 0u64;
    // Without step doubling the macro grid is t0 + k·h (no round-off drift).
    let mut k_grid: u64 = 0;
    while t < opts.t_end - 1e-12 * opts.t_end.abs().max(1.0) {
        ctl.check()?;
        let h_try = if opts.step_doubling.is_none() {
            let target = opts.t0 + (k_grid + 1) as f64 * opts.macro_step;
            if target >= opts.t_end || opts.t_end - target <= 1e-9 * opts.macro_step {
                opts.t_end - t
            } else {
                target - t
            }
        } else {
            h.min(opts.t_end - t)
        };
        match opts.step_doubling {
            None => {
                one_macro_step(c, &mut cache, t, h_try, opts, &mut iters)?;
            }
            Some((rtol, atol)) => {
                let snaps = save_all(c)?;
                let mut big = cache.clone();
                one_macro_step(c, &mut big, t, h_try, opts, &mut iters)?;
                restore_all(c, &snaps)?;
                let mut small = cache.clone();
                one_macro_step(c, &mut small, t, 0.5 * h_try, opts, &mut iters)?;
                one_macro_step(
                    c,
                    &mut small,
                    t + 0.5 * h_try,
                    0.5 * h_try,
                    opts,
                    &mut iters,
                )?;
                let mut err = 0.0f64;
                for (a, b) in big.iter().zip(&small) {
                    for (x, y) in a.iter().zip(b) {
                        err = err.max((x - y).abs() / (atol + rtol * y.abs()));
                    }
                }
                if err > 1.0 {
                    traj.rejected_steps += 1;
                    restore_all(c, &snaps)?;
                    h = 0.5 * h_try;
                    if h < opts.min_macro_step {
                        return Err(SimorphError::Numerical(format!(
                            "macro step underflow at t={t}"
                        )));
                    }
                    continue;
                }
                traj.max_error_estimate = traj.max_error_estimate.max(err);
                cache = small;
                if err < 0.25 {
                    h = (h_try * 2.0).min(opts.macro_step);
                }
            }
        }
        if opts.step_doubling.is_none() {
            k_grid += 1;
            let target = opts.t0 + k_grid as f64 * opts.macro_step;
            t = if target >= opts.t_end || opts.t_end - target <= 1e-9 * opts.macro_step {
                opts.t_end
            } else {
                target
            };
        } else {
            t += h_try;
            if opts.t_end - t <= 1e-12 * opts.t_end.abs().max(1.0) {
                t = opts.t_end;
            }
        }
        traj.macro_steps += 1;
        since_record += 1;
        if since_record >= opts.record_every || t >= opts.t_end {
            record(&mut traj, t, &cache);
            since_record = 0;
        }
    }
    // The final state is always recorded (audit: dropped with record_every > 1
    // when accumulated time stopped just short of t_end).
    if traj.times.last().copied() != Some(t) {
        record(&mut traj, t, &cache);
    }
    traj.fixed_point_iterations = iters;
    Ok(traj)
}

/// Uncertainty propagation hook: build and run one coupling per parameter
/// sample, in parallel.  `build` receives the sample and returns a fresh
/// coupling (couplings own their slaves and are not reusable across samples).
pub fn run_ensemble<F>(
    build: F,
    samples: &[Params],
    opts: &MasterOptions,
    ctl: &ExecutionControl,
) -> Result<Vec<CosimTrajectory>>
where
    F: Fn(&Params) -> Result<Coupling> + Sync,
{
    samples
        .par_iter()
        .map(|s| {
            ctl.check()?;
            let mut c = build(s)?;
            run(&mut c, opts, ctl)
        })
        .collect()
}
