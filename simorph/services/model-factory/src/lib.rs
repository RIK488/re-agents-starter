//! # Model factory (ARC-03)
//!
//! Turns a [`ProblemSpec`] plus the component registry into an explicit,
//! dimensionally checked model assembly, and turns user-typed equations
//! into executable models:
//!
//! * [`dimension`] – dimensional inference over arithmetic expressions
//!   (`k*(T - T_env)/C` → `K/s`), used to reject inconsistent equations
//!   before anything is computed (REQ-MOD-3).
//! * [`expression_model`] – [`ExpressionModel`], a
//!   `simorph_simulation::DynamicalModel` built from lines such as
//!   `d(x)/dt = -k*x` and `y = 2*x`, evaluated with the validation engine's
//!   safe expression evaluator.
//! * [`assembly`] – [`ModelFactory::assemble`]: chain planning through the
//!   registry, connection compatibility with unit conversion factors, one
//!   [`ModelDescriptor`] per step plus a composite descriptor, and the
//!   dimensional report of objectives/constraints.
//! * [`render`] – Markdown for the UI's "Espace Modélisation".

pub mod assembly;
pub mod dimension;
pub mod expression_model;
pub mod render;

pub use assembly::*;
pub use dimension::*;
pub use expression_model::*;
pub use render::*;
