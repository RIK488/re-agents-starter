//! The continuous-time model contract.

use indexmap::IndexMap;
use simorph_core::{ReferenceCase, Result};

/// Named parameters (and, in co-simulation, inputs) of a model.
pub type Params = IndexMap<String, f64>;

/// Helper to read a parameter with a clear error.
pub fn param(p: &Params, name: &str) -> Result<f64> {
    p.get(name).copied().ok_or_else(|| {
        simorph_core::SimorphError::InvalidInput(format!("missing parameter `{name}`"))
    })
}

/// Helper to read a parameter with a default.
pub fn param_or(p: &Params, name: &str, default: f64) -> f64 {
    p.get(name).copied().unwrap_or(default)
}

/// What the driver does after an event handler ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventAction {
    /// Keep integrating from the (possibly modified) state.
    Continue,
    /// Stop the simulation at the event time.
    Terminate,
}

/// Which sign changes of an event function count as an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDirection {
    /// Any sign change.
    Any,
    /// Only negative -> positive.
    Rising,
    /// Only positive -> negative.
    Falling,
}

/// A continuous-time dynamical system `dx/dt = f(t, x; p)` with outputs
/// `y = g(t, x; p)` and optional zero-crossing events.
pub trait DynamicalModel: Send + Sync {
    /// Stable, unique model name (used in manifests and provenance).
    fn name(&self) -> &str;

    /// Number of state variables.
    fn state_dim(&self) -> usize;

    /// `(name, unit)` of each state variable, in state order.
    fn state_names(&self) -> Vec<(String, String)>;

    /// `(name, unit)` of each parameter the model reads from `Params`.
    fn parameter_names(&self) -> Vec<(String, String)>;

    /// `(name, unit)` of each output produced by [`outputs`](Self::outputs).
    fn output_names(&self) -> Vec<(String, String)> {
        vec![]
    }

    /// Default initial state for the given parameters.
    fn initial_state(&self, params: &Params) -> Vec<f64>;

    /// Right-hand side: fill `dx` with `f(t, x; p)`.
    fn rhs(&self, t: f64, x: &[f64], dx: &mut [f64], params: &Params) -> Result<()>;

    /// Output map `y = g(t, x; p)`.  Default: no outputs.
    fn outputs(&self, _t: f64, _x: &[f64], _params: &Params) -> IndexMap<String, f64> {
        IndexMap::new()
    }

    /// Number of event (zero-crossing) functions.
    fn num_events(&self) -> usize {
        0
    }

    /// Value of event function `idx`; an event occurs where it crosses zero.
    fn event_value(&self, _idx: usize, _t: f64, _x: &[f64], _params: &Params) -> f64 {
        0.0
    }

    /// Direction filter for event `idx`.
    fn event_direction(&self, _idx: usize) -> EventDirection {
        EventDirection::Any
    }

    /// Handler invoked at the located event time.  May modify the state
    /// (e.g. reverse a velocity).
    fn handle_event(&self, _idx: usize, _t: f64, _x: &mut [f64], _params: &Params) -> EventAction {
        EventAction::Continue
    }

    /// Analytic Jacobian `∂f/∂x` (row-major `n×n`).  Return `None` to let the
    /// solver use finite differences.
    fn jacobian(
        &self,
        _t: f64,
        _x: &[f64],
        _params: &Params,
        _jac: &mut [f64],
    ) -> Option<Result<()>> {
        None
    }

    /// Documented modelling assumptions (copied into the manifest).
    fn assumptions(&self) -> Vec<String> {
        vec![]
    }

    /// Validity ranges `(param, (low, high))` copied into the manifest.
    fn validity_ranges(&self) -> Vec<(String, (f64, f64))> {
        vec![]
    }

    /// Reference cases copied into the manifest validation protocol.
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        vec![]
    }

    /// Domain tags (e.g. `"aerospace"`), free-form.
    fn domains(&self) -> Vec<String> {
        vec![]
    }
}

/// Finite-difference Jacobian `∂f/∂x`, row-major, forward differences.
/// Returns the number of RHS evaluations used.
pub fn numerical_jacobian(
    model: &dyn DynamicalModel,
    t: f64,
    x: &[f64],
    params: &Params,
    jac: &mut [f64],
) -> Result<usize> {
    let n = x.len();
    let mut f0 = vec![0.0; n];
    model.rhs(t, x, &mut f0, params)?;
    let mut xp = x.to_vec();
    let mut fp = vec![0.0; n];
    for j in 0..n {
        let h = 1e-7 * x[j].abs().max(1.0);
        xp[j] = x[j] + h;
        model.rhs(t, &xp, &mut fp, params)?;
        xp[j] = x[j];
        for i in 0..n {
            jac[i * n + j] = (fp[i] - f0[i]) / h;
        }
    }
    Ok(n + 1)
}
