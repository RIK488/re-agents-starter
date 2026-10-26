//! # simorph-cosimulation
//!
//! Runs several simulators together (§3.5 of the specification).
//!
//! * [`slave::CosimSlave`] – FMI-like step/exchange contract every
//!   participant implements (`initialize`, `set_input`, `do_step`,
//!   `get_output`, optional state save/restore for iterative masters).
//! * [`coupling::Coupling`] – the set of slaves and the signal connections
//!   between them.  Units are converted automatically through
//!   `simorph-units`; a dimensional mismatch is rejected when the coupling is
//!   built; algebraic loops are detected by strongly-connected-component
//!   analysis of the direct-feedthrough graph.
//! * [`master`] – Jacobi (parallel exchange), Gauss–Seidel (ordered) and
//!   iterative Gauss–Seidel (fixed-point iteration with rollback) masters,
//!   optional step-doubling error control, consistency checks, and
//!   [`master::run_ensemble`] for uncertainty propagation.
//! * [`ode_slave::OdeSlave`] – wraps any `simorph_simulation::DynamicalModel`.
//! * [`algebraic::AlgebraicSlave`] – a stateless slave defined by a closure.

#![allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]

pub mod algebraic;
pub mod coupling;
pub mod master;
pub mod ode_slave;
pub mod slave;

pub use algebraic::AlgebraicSlave;
pub use coupling::{Coupling, CouplingBuilder};
pub use master::{run, run_ensemble, CosimTrajectory, MasterAlgorithm, MasterOptions};
pub use ode_slave::OdeSlave;
pub use slave::{CosimSlave, Snapshot};
