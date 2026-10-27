//! # simorph-evolution
//!
//! Subsystem 10 of SIMORPH: evolution of algorithms, programs and agents.
//!
//! * [`expr`] / [`gp`] — typed expression trees and an island genetic
//!   programming engine with lexicographic parsimony, a Pareto archive on
//!   (error, size), Nelder–Mead constant optimisation and **dimensional
//!   constraints** derived from SIMORPH units ([`gp::DimensionMode`]).
//! * [`qd`] — MAP-Elites quality-diversity archive with a novelty bonus.
//! * [`program`] — evolution of real Python programs: [`program::Mutator`]
//!   trait, deterministic syntactic mutator, optional LLM mutator (HTTP,
//!   configured by environment), sandboxed execution (network namespace,
//!   restricted builtins, timeouts) and evaluation by an independent Rust
//!   scorer on a sealed instance set.
//! * [`binpacking`] — combinatorial benchmark with known optima used to
//!   demonstrate program evolution.
//! * [`discovery`] — scientific-law discovery from simulated experiments
//!   (Kepler's third law from integrated orbits, pendulum period and its
//!   amplitude correction) with held-out validation, dimensional checks,
//!   evidence-level claims and provenance.
//!
//! Inspired by ShinkaEvolve, the Darwin Gödel Machine and AlphaEvolve
//! (population of programs, archive, evaluator outside the candidate's
//! reach), implemented natively; no code from those projects is used.

pub mod binpacking;
pub mod discovery;
pub mod expr;
pub mod gp;
pub mod program;
pub mod qd;

pub use expr::*;
pub use gp::*;
