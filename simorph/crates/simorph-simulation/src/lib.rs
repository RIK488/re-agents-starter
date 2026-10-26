//! # simorph-simulation
//!
//! Numerical simulation kernels.  Nothing in this crate knows about any
//! physical domain; it provides the machinery that domain plugins use:
//!
//! * [`model::DynamicalModel`] – the contract a continuous-time model fulfils
//!   (right-hand side, outputs, parameters with units, optional events and
//!   analytic Jacobian).
//! * [`integrators`] – explicit Euler, classical RK4, Dormand–Prince RK45
//!   with adaptive error control, backward Euler and implicit trapezoidal
//!   (Newton iteration with a numerical Jacobian) for stiff systems.
//! * [`simulate`] – the driver: integrates a model over a time span,
//!   honours cancellation/deadlines, detects zero-crossing events and
//!   records a [`Trajectory`] with solver statistics.
//! * [`des`] – a deterministic discrete-event kernel.
//! * [`agents`] – a minimal, seedable multi-agent step scheduler.
//! * [`component`] – [`OdeComponent`], which exposes any model through the
//!   universal `simorph_core::Component` contract.
//!
//! Every integrator in this crate is tested against analytic solutions and
//! its measured convergence order (see the `tests` modules).

#![allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]

pub mod agents;
pub mod component;
pub mod des;
pub mod integrators;
pub mod linalg;
pub mod model;
pub mod simulate;

pub use component::OdeComponent;
pub use integrators::{Solver, SolverOptions};
pub use model::{DynamicalModel, EventAction, EventDirection, Params};
pub use simulate::{simulate, SolverStats, Trajectory};
