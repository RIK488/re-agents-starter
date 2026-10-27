//! Time-stepping schemes.
//!
//! All steppers share the signature of [`one_step`]: advance the state from
//! `t` to `t + h` and, for embedded methods, return an error estimate.
//! Fixed-step explicit methods (Euler, RK4), the embedded adaptive
//! Dormand–Prince 5(4) pair, and two A-stable implicit methods (backward
//! Euler, trapezoidal rule) solved by Newton iteration are provided.

use crate::linalg::solve_dense;
use crate::model::{numerical_jacobian, DynamicalModel, Params};
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};

/// Available integration schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Solver {
    /// First-order explicit Euler (fixed step).  Reference method only.
    Euler,
    /// Classical fourth-order Runge–Kutta (fixed step).
    Rk4,
    /// Dormand–Prince 5(4) embedded pair with adaptive step (default).
    #[default]
    Rk45,
    /// Backward (implicit) Euler, first order, L-stable; for stiff systems.
    BackwardEuler,
    /// Implicit trapezoidal rule, second order, A-stable; for stiff systems.
    Trapezoidal,
}

impl Solver {
    pub fn is_adaptive(self) -> bool {
        matches!(self, Solver::Rk45)
    }
    pub fn is_implicit(self) -> bool {
        matches!(self, Solver::BackwardEuler | Solver::Trapezoidal)
    }
    /// Theoretical global order of accuracy.
    pub fn order(self) -> u32 {
        match self {
            Solver::Euler | Solver::BackwardEuler => 1,
            Solver::Trapezoidal => 2,
            Solver::Rk4 => 4,
            Solver::Rk45 => 5,
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "euler" => Solver::Euler,
            "rk4" => Solver::Rk4,
            "rk45" | "dopri5" | "dormand_prince" => Solver::Rk45,
            "backward_euler" | "implicit_euler" => Solver::BackwardEuler,
            "trapezoidal" | "trapezoid" => Solver::Trapezoidal,
            other => {
                return Err(SimorphError::InvalidInput(format!(
                    "unknown solver `{other}`"
                )))
            }
        })
    }
}

/// Solver configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverOptions {
    pub solver: Solver,
    /// Fixed step (fixed-step methods) or initial step (adaptive).
    pub dt: f64,
    /// Relative tolerance (adaptive).
    pub rtol: f64,
    /// Absolute tolerance (adaptive).
    pub atol: f64,
    pub dt_min: f64,
    pub dt_max: f64,
    /// Maximum number of accepted steps before giving up.
    pub max_steps: usize,
    /// Record every n-th accepted step (1 = all).
    pub record_every: usize,
    /// Newton tolerance for implicit methods (relative to state scale).
    pub newton_tol: f64,
    pub newton_max_iter: usize,
    /// Relative tolerance on the event time location.
    pub event_tol: f64,
}

impl Default for SolverOptions {
    fn default() -> Self {
        Self {
            solver: Solver::Rk45,
            dt: 1e-2,
            rtol: 1e-6,
            atol: 1e-9,
            dt_min: 1e-12,
            dt_max: f64::INFINITY,
            max_steps: 10_000_000,
            record_every: 1,
            newton_tol: 1e-10,
            newton_max_iter: 50,
            event_tol: 1e-10,
        }
    }
}

impl SolverOptions {
    pub fn new(solver: Solver) -> Self {
        Self {
            solver,
            ..Default::default()
        }
    }
    pub fn with_dt(mut self, dt: f64) -> Self {
        self.dt = dt;
        self
    }
    pub fn with_tolerances(mut self, rtol: f64, atol: f64) -> Self {
        self.rtol = rtol;
        self.atol = atol;
        self
    }
    pub fn with_dt_max(mut self, dt_max: f64) -> Self {
        self.dt_max = dt_max;
        self
    }
    pub fn with_record_every(mut self, n: usize) -> Self {
        self.record_every = n.max(1);
        self
    }
}

/// Work counters updated by every stepper, plus the first-same-as-last
/// (FSAL) cache of the Dormand–Prince pair.
#[derive(Debug, Default, Clone)]
pub struct Work {
    pub rhs_evals: u64,
    pub newton_iters: u64,
    pub jacobian_evals: u64,
    /// Up to two cached derivative evaluations `(t, x, f(t, x))`: the first
    /// stage of the last attempt (reused after a rejection) and the last
    /// stage of the last attempt (reused as the first stage of the next
    /// accepted step).
    fsal: Vec<(f64, Vec<f64>, Vec<f64>)>,
}

impl Work {
    fn cached(&self, t: f64, x: &[f64]) -> Option<&Vec<f64>> {
        self.fsal
            .iter()
            .find(|(tc, xc, _)| *tc == t && xc.as_slice() == x)
            .map(|(_, _, f)| f)
    }
    fn remember(&mut self, t: f64, x: &[f64], f: &[f64]) {
        if self.cached(t, x).is_some() {
            return;
        }
        if self.fsal.len() >= 2 {
            self.fsal.remove(0);
        }
        self.fsal.push((t, x.to_vec(), f.to_vec()));
    }
}

/// Result of one step attempt.
pub struct StepOutcome {
    /// State at `t + h`.
    pub x_new: Vec<f64>,
    /// Scaled error estimate (`<= 1` means acceptable), if the method has one.
    pub err: Option<f64>,
}

/// Advance one step of size `h` with the chosen scheme.
pub fn one_step(
    model: &dyn DynamicalModel,
    opts: &SolverOptions,
    t: f64,
    x: &[f64],
    h: f64,
    params: &Params,
    work: &mut Work,
) -> Result<StepOutcome> {
    match opts.solver {
        Solver::Euler => euler(model, t, x, h, params, work),
        Solver::Rk4 => rk4(model, t, x, h, params, work),
        Solver::Rk45 => dopri5(model, opts, t, x, h, params, work),
        Solver::BackwardEuler => implicit(model, opts, t, x, h, params, work, 1.0),
        Solver::Trapezoidal => implicit(model, opts, t, x, h, params, work, 0.5),
    }
}

fn eval(
    model: &dyn DynamicalModel,
    t: f64,
    x: &[f64],
    dx: &mut [f64],
    p: &Params,
    w: &mut Work,
) -> Result<()> {
    w.rhs_evals += 1;
    model.rhs(t, x, dx, p)
}

fn euler(
    m: &dyn DynamicalModel,
    t: f64,
    x: &[f64],
    h: f64,
    p: &Params,
    w: &mut Work,
) -> Result<StepOutcome> {
    let n = x.len();
    let mut k = vec![0.0; n];
    eval(m, t, x, &mut k, p, w)?;
    let x_new = (0..n).map(|i| x[i] + h * k[i]).collect();
    Ok(StepOutcome { x_new, err: None })
}

fn rk4(
    m: &dyn DynamicalModel,
    t: f64,
    x: &[f64],
    h: f64,
    p: &Params,
    w: &mut Work,
) -> Result<StepOutcome> {
    let n = x.len();
    let mut k1 = vec![0.0; n];
    let mut k2 = vec![0.0; n];
    let mut k3 = vec![0.0; n];
    let mut k4 = vec![0.0; n];
    let mut tmp = vec![0.0; n];
    eval(m, t, x, &mut k1, p, w)?;
    for i in 0..n {
        tmp[i] = x[i] + 0.5 * h * k1[i];
    }
    eval(m, t + 0.5 * h, &tmp, &mut k2, p, w)?;
    for i in 0..n {
        tmp[i] = x[i] + 0.5 * h * k2[i];
    }
    eval(m, t + 0.5 * h, &tmp, &mut k3, p, w)?;
    for i in 0..n {
        tmp[i] = x[i] + h * k3[i];
    }
    eval(m, t + h, &tmp, &mut k4, p, w)?;
    let x_new = (0..n)
        .map(|i| x[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]))
        .collect();
    Ok(StepOutcome { x_new, err: None })
}

// Dormand–Prince 5(4) Butcher tableau.
const C: [f64; 7] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0, 1.0];
const A: [[f64; 6]; 7] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0 / 5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0, 0.0],
    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0, 0.0],
    [
        19372.0 / 6561.0,
        -25360.0 / 2187.0,
        64448.0 / 6561.0,
        -212.0 / 729.0,
        0.0,
        0.0,
    ],
    [
        9017.0 / 3168.0,
        -355.0 / 33.0,
        46732.0 / 5247.0,
        49.0 / 176.0,
        -5103.0 / 18656.0,
        0.0,
    ],
    [
        35.0 / 384.0,
        0.0,
        500.0 / 1113.0,
        125.0 / 192.0,
        -2187.0 / 6784.0,
        11.0 / 84.0,
    ],
];
/// 5th-order weights.
const B5: [f64; 7] = [
    35.0 / 384.0,
    0.0,
    500.0 / 1113.0,
    125.0 / 192.0,
    -2187.0 / 6784.0,
    11.0 / 84.0,
    0.0,
];
/// 4th-order weights (embedded).
const B4: [f64; 7] = [
    5179.0 / 57600.0,
    0.0,
    7571.0 / 16695.0,
    393.0 / 640.0,
    -92097.0 / 339200.0,
    187.0 / 2100.0,
    1.0 / 40.0,
];

fn dopri5(
    m: &dyn DynamicalModel,
    opts: &SolverOptions,
    t: f64,
    x: &[f64],
    h: f64,
    p: &Params,
    w: &mut Work,
) -> Result<StepOutcome> {
    let n = x.len();
    let mut k = vec![vec![0.0; n]; 7];
    let mut tmp = vec![0.0; n];
    // FSAL: the last stage of an accepted step is f(t+h, x_new), i.e. the
    // first stage of the next step (6 evaluations per accepted step).
    if let Some(f) = w.cached(t, x) {
        k[0].copy_from_slice(f);
    } else {
        eval(m, t, x, &mut k[0], p, w)?;
        let k0 = k[0].clone();
        w.remember(t, x, &k0);
    }
    for s in 1..7 {
        for i in 0..n {
            let mut acc = x[i];
            for (j, a) in A[s].iter().enumerate().take(s) {
                acc += h * a * k[j][i];
            }
            tmp[i] = acc;
        }
        let (head, tail) = k.split_at_mut(s);
        let _ = head;
        eval(m, t + C[s] * h, &tmp, &mut tail[0], p, w)?;
    }
    let mut x_new = vec![0.0; n];
    let mut err = 0.0f64;
    for i in 0..n {
        let mut x5 = x[i];
        let mut x4 = x[i];
        for s in 0..7 {
            x5 += h * B5[s] * k[s][i];
            x4 += h * B4[s] * k[s][i];
        }
        x_new[i] = x5;
        let sc = opts.atol + opts.rtol * x[i].abs().max(x5.abs());
        let e = (x5 - x4) / sc;
        err += e * e;
    }
    let err = (err / n.max(1) as f64).sqrt();
    // Stage 7 was evaluated at x + h Σ a_7j k_j, which equals x5 because the
    // last row of A is b5; cache it only when the arithmetic is bit-identical.
    if tmp == x_new {
        let k6 = k[6].clone();
        w.remember(t + h, &x_new, &k6);
    }
    Ok(StepOutcome {
        x_new,
        err: Some(err),
    })
}

/// θ-method: `x_{n+1} = x_n + h [ (1-θ) f(t_n, x_n) + θ f(t_{n+1}, x_{n+1}) ]`.
/// θ = 1 is backward Euler, θ = 0.5 is the trapezoidal rule.
#[allow(clippy::too_many_arguments)]
fn implicit(
    m: &dyn DynamicalModel,
    opts: &SolverOptions,
    t: f64,
    x: &[f64],
    h: f64,
    p: &Params,
    w: &mut Work,
    theta: f64,
) -> Result<StepOutcome> {
    let n = x.len();
    let mut f0 = vec![0.0; n];
    eval(m, t, x, &mut f0, p, w)?;
    // Predictor: explicit Euler.
    let mut y: Vec<f64> = (0..n).map(|i| x[i] + h * f0[i]).collect();
    let mut fy = vec![0.0; n];
    let mut jac = vec![0.0; n * n];
    let mut g = vec![0.0; n];
    let mut mat = vec![0.0; n * n];
    let t1 = t + h;
    let mut converged = false;
    for _ in 0..opts.newton_max_iter {
        w.newton_iters += 1;
        eval(m, t1, &y, &mut fy, p, w)?;
        for i in 0..n {
            g[i] = y[i] - x[i] - h * ((1.0 - theta) * f0[i] + theta * fy[i]);
        }
        let gnorm = g.iter().map(|v| v * v).sum::<f64>().sqrt();
        let scale = 1.0 + y.iter().map(|v| v.abs()).fold(0.0, f64::max);
        if gnorm <= opts.newton_tol * scale {
            converged = true;
            break;
        }
        match m.jacobian(t1, &y, p, &mut jac) {
            Some(r) => r?,
            None => {
                w.rhs_evals += numerical_jacobian(m, t1, &y, p, &mut jac)? as u64;
            }
        }
        w.jacobian_evals += 1;
        for i in 0..n {
            for j in 0..n {
                mat[i * n + j] = -h * theta * jac[i * n + j] + if i == j { 1.0 } else { 0.0 };
            }
        }
        let mut rhs: Vec<f64> = g.iter().map(|v| -v).collect();
        solve_dense(&mut mat, &mut rhs)?;
        let mut dnorm = 0.0f64;
        for i in 0..n {
            y[i] += rhs[i];
            dnorm = dnorm.max(rhs[i].abs());
        }
        if dnorm <= opts.newton_tol * scale {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(SimorphError::Numerical(format!(
            "Newton iteration did not converge at t={t1} (h={h})"
        )));
    }
    Ok(StepOutcome {
        x_new: y,
        err: None,
    })
}
