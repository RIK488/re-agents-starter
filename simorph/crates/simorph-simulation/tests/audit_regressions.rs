//! Regression tests for the 2026-10-27 engines audit.

use simorph_core::{ExecutionControl, Result};
use simorph_simulation::model::{EventAction, EventDirection};
use simorph_simulation::{simulate, DynamicalModel, Params, Solver, SolverOptions};

/// x' = 1, event x − 1 = 0 (terminating).
struct Ramp;
impl DynamicalModel for Ramp {
    fn name(&self) -> &str {
        "ramp"
    }
    fn state_dim(&self) -> usize {
        1
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "1".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn initial_state(&self, _p: &Params) -> Vec<f64> {
        vec![0.0]
    }
    fn rhs(&self, _t: f64, _x: &[f64], dx: &mut [f64], _p: &Params) -> Result<()> {
        dx[0] = 1.0;
        Ok(())
    }
    fn num_events(&self) -> usize {
        1
    }
    fn event_value(&self, _i: usize, _t: f64, x: &[f64], _p: &Params) -> f64 {
        x[0] - 1.0
    }
    fn event_direction(&self, _i: usize) -> EventDirection {
        EventDirection::Any
    }
    fn handle_event(&self, _i: usize, _t: f64, _x: &mut [f64], _p: &Params) -> EventAction {
        EventAction::Terminate
    }
}

/// x'' = −x.
struct Osc;
impl DynamicalModel for Osc {
    fn name(&self) -> &str {
        "osc"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "1".into()), ("v".into(), "1".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn initial_state(&self, _p: &Params) -> Vec<f64> {
        vec![1.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], _p: &Params) -> Result<()> {
        dx[0] = x[1];
        dx[1] = -x[0];
        Ok(())
    }
}

#[test]
fn audit_event_exactly_on_step_boundary_is_detected() {
    // Audit probe: dt = 0.25 put the crossing x = 1 exactly at t = 1 → missed.
    for dt in [0.25, 0.3, 0.5] {
        let tr = simulate(
            &Ramp,
            &Params::new(),
            None,
            (0.0, 3.0),
            &SolverOptions::new(Solver::Rk4).with_dt(dt),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(tr.terminated_by_event, "dt = {dt}: event missed");
        assert!(
            (tr.final_time() - 1.0).abs() < 1e-9,
            "dt = {dt}: t = {}",
            tr.final_time()
        );
        assert_eq!(tr.events.len(), 1);
    }
}

#[test]
fn audit_fixed_step_has_no_sliver_step() {
    // Audit probe: RK4 dt = 1e-3 to t = 2000 took 2 000 001 steps.
    let tr = simulate(
        &Osc,
        &Params::new(),
        None,
        (0.0, 200.0),
        &SolverOptions::new(Solver::Rk4).with_dt(1e-3),
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(tr.stats.steps, 200_000);
    assert_eq!(tr.final_time(), 200.0);
    assert!(!tr.stats.error_estimated);
}

#[test]
fn audit_dopri5_reuses_last_stage() {
    let tr = simulate(
        &Osc,
        &Params::new(),
        None,
        (0.0, 20.0),
        &SolverOptions::new(Solver::Rk45).with_tolerances(1e-8, 1e-10),
        &ExecutionControl::default(),
    )
    .unwrap();
    let attempts = tr.stats.steps + tr.stats.rejected_steps;
    // FSAL: 1 initial evaluation + 6 per accepted step + 6 per rejected attempt.
    assert_eq!(
        tr.stats.rhs_evals,
        1 + 6 * attempts,
        "steps {} rejected {}",
        tr.stats.steps,
        tr.stats.rejected_steps
    );
    assert!(tr.stats.error_estimated);
    // accuracy unchanged by the cache
    let x = tr.states.last().unwrap()[0];
    assert!((x - 20f64.cos()).abs() < 1e-6);
}
