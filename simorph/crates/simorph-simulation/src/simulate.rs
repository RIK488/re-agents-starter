//! Simulation driver: step control, event location, recording.

use crate::integrators::{one_step, SolverOptions, Work};
use crate::model::{DynamicalModel, EventAction, EventDirection, Params};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};
use std::time::Instant;

/// Solver statistics attached to every trajectory.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SolverStats {
    /// Whether `max_error_estimate` is a real (local) error estimate.  Fixed-step
    /// methods (Euler, RK4, implicit θ-methods) have none: their estimate is
    /// reported as 0 with this flag false, and must not be read as "no error".
    #[serde(default)]
    pub error_estimated: bool,
    pub steps: u64,
    pub rejected_steps: u64,
    pub rhs_evals: u64,
    pub newton_iters: u64,
    pub jacobian_evals: u64,
    /// Largest scaled error estimate among accepted steps (adaptive only).
    pub max_error_estimate: f64,
    pub events: u64,
    pub wall_time_ms: u64,
    pub final_dt: f64,
}

/// A located event: `(event index, time)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocatedEvent {
    pub index: usize,
    pub time: f64,
}

/// Recorded solution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    pub times: Vec<f64>,
    /// `states[k]` is the state vector at `times[k]`.
    pub states: Vec<Vec<f64>>,
    /// Output name -> series aligned with `times`.
    pub outputs: IndexMap<String, Vec<f64>>,
    pub state_names: Vec<(String, String)>,
    pub stats: SolverStats,
    pub events: Vec<LocatedEvent>,
    pub terminated_by_event: bool,
}

impl Trajectory {
    pub fn final_time(&self) -> f64 {
        *self
            .times
            .last()
            .expect("trajectory has at least one point")
    }
    pub fn final_state(&self) -> &[f64] {
        self.states
            .last()
            .expect("trajectory has at least one point")
    }
    /// Series of state component `i`.
    pub fn state_series(&self, i: usize) -> Vec<f64> {
        self.states.iter().map(|s| s[i]).collect()
    }
    /// Index of the state named `name`.
    pub fn state_index(&self, name: &str) -> Option<usize> {
        self.state_names.iter().position(|(n, _)| n == name)
    }
    /// Linear interpolation of the state at time `t` (clamped to the range).
    pub fn interpolate(&self, t: f64) -> Vec<f64> {
        let n = self.times.len();
        if n == 1 || t <= self.times[0] {
            return self.states[0].clone();
        }
        if t >= self.times[n - 1] {
            return self.states[n - 1].clone();
        }
        let k = match self.times.binary_search_by(|v| v.partial_cmp(&t).unwrap()) {
            Ok(i) => return self.states[i].clone(),
            Err(i) => i,
        };
        let (t0, t1) = (self.times[k - 1], self.times[k]);
        let w = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
        self.states[k - 1]
            .iter()
            .zip(&self.states[k])
            .map(|(a, b)| a + w * (b - a))
            .collect()
    }
    pub fn len(&self) -> usize {
        self.times.len()
    }
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }
}

struct Recorder<'a> {
    model: &'a dyn DynamicalModel,
    params: &'a Params,
    traj: Trajectory,
}

impl<'a> Recorder<'a> {
    fn push(&mut self, t: f64, x: &[f64]) {
        self.traj.times.push(t);
        self.traj.states.push(x.to_vec());
        let out = self.model.outputs(t, x, self.params);
        for (k, v) in out {
            self.traj.outputs.entry(k).or_default().push(v);
        }
    }
}

fn sign_change(dir: EventDirection, a: f64, b: f64) -> bool {
    if a == 0.0 || a.is_nan() || b.is_nan() {
        // A value exactly at zero at the *start* of a step carries no sign
        // information (this is what allows an event handler to reset a state
        // onto the event surface without immediately re-triggering, and what
        // prevents a crossing that ended exactly on a step boundary from
        // being detected twice).
        return false;
    }
    // A step that *ends* exactly on the surface is a crossing (audit: such
    // events used to be missed).
    match dir {
        EventDirection::Any => b == 0.0 || a * b < 0.0,
        EventDirection::Rising => a < 0.0 && b >= 0.0,
        EventDirection::Falling => a > 0.0 && b <= 0.0,
    }
}

/// Integrate `model` from `t_span.0` to `t_span.1` starting at `x0`
/// (or the model's default initial state when `x0` is `None`).
pub fn simulate(
    model: &dyn DynamicalModel,
    params: &Params,
    x0: Option<&[f64]>,
    t_span: (f64, f64),
    opts: &SolverOptions,
    ctl: &ExecutionControl,
) -> Result<Trajectory> {
    let start = Instant::now();
    let (t0, t_end) = t_span;
    if !(t_end > t0) {
        return Err(SimorphError::InvalidInput(format!(
            "t_end ({t_end}) must exceed t0 ({t0})"
        )));
    }
    if !(opts.dt > 0.0) {
        return Err(SimorphError::InvalidInput("dt must be positive".into()));
    }
    let n = model.state_dim();
    let mut x: Vec<f64> = match x0 {
        Some(v) => v.to_vec(),
        None => model.initial_state(params),
    };
    if x.len() != n {
        return Err(SimorphError::InvalidInput(format!(
            "initial state has {} components, model `{}` expects {n}",
            x.len(),
            model.name()
        )));
    }
    let mut rec = Recorder {
        model,
        params,
        traj: Trajectory {
            times: vec![],
            states: vec![],
            outputs: IndexMap::new(),
            state_names: model.state_names(),
            stats: SolverStats::default(),
            events: vec![],
            terminated_by_event: false,
        },
    };
    let mut work = Work::default();
    let mut stats = SolverStats::default();
    let ne = model.num_events();
    let mut ev_start: Vec<f64> = (0..ne)
        .map(|i| model.event_value(i, t0, &x, params))
        .collect();
    let mut ev_end = vec![0.0; ne];

    let mut t = t0;
    let mut h = opts.dt.min(opts.dt_max).min(t_end - t0);
    rec.push(t, &x);
    let mut accepted_since_record = 0usize;
    // Fixed-step methods follow the grid t_k = t_grid0 + k·dt (no round-off
    // accumulation, no sliver final step); the grid restarts after an event.
    let fixed = !matches!(opts.solver, crate::integrators::Solver::Rk45);
    let mut grid0 = t0;
    let mut k_grid: u64 = 0;

    while t < t_end {
        ctl.check()?;
        if stats.steps + stats.rejected_steps >= opts.max_steps as u64 {
            return Err(SimorphError::Numerical(format!(
                "max_steps ({}) exceeded at t={t}",
                opts.max_steps
            )));
        }
        if h < opts.dt_min {
            return Err(SimorphError::Numerical(format!(
                "step size underflow at t={t} (h={h})"
            )));
        }
        let h_try = if fixed {
            let dt = opts.dt.min(opts.dt_max);
            let mut target = grid0 + (k_grid + 1) as f64 * dt;
            if target >= t_end || (t_end - target) <= 1e-9 * dt {
                target = t_end;
            }
            target - t
        } else {
            h.min(t_end - t)
        };
        let out = one_step(model, opts, t, &x, h_try, params, &mut work)?;
        if out.x_new.iter().any(|v| !v.is_finite()) {
            return Err(SimorphError::Numerical(format!(
                "non-finite state produced by `{}` at t={}",
                model.name(),
                t + h_try
            )));
        }
        // Adaptive acceptance.
        if let Some(err) = out.err {
            if err > 1.0 {
                stats.rejected_steps += 1;
                let fac = (0.9 * err.powf(-0.2)).clamp(0.2, 1.0);
                h = h_try * fac;
                continue;
            }
            stats.max_error_estimate = stats.max_error_estimate.max(err);
            let fac = if err == 0.0 {
                5.0
            } else {
                (0.9 * err.powf(-0.2)).clamp(0.2, 5.0)
            };
            h = (h_try * fac).min(opts.dt_max);
        }
        let mut t_new = if fixed {
            let dt = opts.dt.min(opts.dt_max);
            let target = grid0 + (k_grid + 1) as f64 * dt;
            if target >= t_end || (t_end - target) <= 1e-9 * dt {
                t_end
            } else {
                target
            }
        } else {
            t + h_try
        };
        let mut x_new = out.x_new;

        let mut grid_reset = false;
        // Event detection.
        let mut fired: Option<(usize, f64, Vec<f64>)> = None;
        if ne > 0 {
            for i in 0..ne {
                ev_end[i] = model.event_value(i, t_new, &x_new, params);
            }
            for i in 0..ne {
                if sign_change(model.event_direction(i), ev_start[i], ev_end[i]) {
                    // Bisection on the step length from (t, x).
                    let (mut lo, mut hi) = (0.0f64, h_try);
                    let mut x_hi = x_new.clone();
                    let tol = opts.event_tol * t.abs().max(1.0);
                    for _ in 0..200 {
                        if hi - lo <= tol {
                            break;
                        }
                        let mid = 0.5 * (lo + hi);
                        let xm = one_step(model, opts, t, &x, mid, params, &mut work)?.x_new;
                        let vm = model.event_value(i, t + mid, &xm, params);
                        if sign_change(model.event_direction(i), ev_start[i], vm) {
                            hi = mid;
                            x_hi = xm;
                        } else {
                            lo = mid;
                        }
                    }
                    let t_ev = t + hi;
                    if fired.as_ref().is_none_or(|(_, te, _)| t_ev < *te) {
                        fired = Some((i, t_ev, x_hi));
                    }
                }
            }
        }
        if let Some((idx, t_ev, mut x_ev)) = fired {
            stats.events += 1;
            let action = model.handle_event(idx, t_ev, &mut x_ev, params);
            rec.traj.events.push(LocatedEvent {
                index: idx,
                time: t_ev,
            });
            t_new = t_ev;
            x_new = x_ev;
            if action == EventAction::Terminate {
                rec.push(t_new, &x_new);
                rec.traj.terminated_by_event = true;
                t = t_new;
                x = x_new;
                break;
            }
            rec.push(t_new, &x_new);
            accepted_since_record = 0;
            // Re-evaluate event functions from the post-event state.
            for i in 0..ne {
                ev_start[i] = model.event_value(i, t_new, &x_new, params);
            }
            // Do not let the adaptive step grow across the discontinuity.
            h = h.min(opts.dt);
            grid_reset = true;
        } else {
            std::mem::swap(&mut ev_start, &mut ev_end);
            accepted_since_record += 1;
            if accepted_since_record >= opts.record_every || t_new >= t_end {
                rec.push(t_new, &x_new);
                accepted_since_record = 0;
            }
        }
        stats.steps += 1;
        stats.final_dt = h_try;
        if grid_reset {
            grid0 = t_new;
            k_grid = 0;
        } else {
            k_grid += 1;
        }
        t = t_new;
        x = x_new;
    }
    if rec.traj.times.last().copied() != Some(t) {
        rec.push(t, &x);
    }
    stats.rhs_evals = work.rhs_evals;
    stats.error_estimated = !fixed;
    stats.newton_iters = work.newton_iters;
    stats.jacobian_evals = work.jacobian_evals;
    stats.wall_time_ms = start.elapsed().as_millis() as u64;
    rec.traj.stats = stats;
    Ok(rec.traj)
}
