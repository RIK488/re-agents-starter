//! # simorph-uncertainty
//!
//! Uncertainty quantification for SIMORPH (§3.8, §8):
//!
//! * [`rng`] – deterministic seeded random numbers (ChaCha20).
//! * [`sampling`] – sampling from `simorph_core::Uncertainty` models and
//!   space-filling designs (Monte Carlo, Latin hypercube, Sobol, Halton,
//!   full factorial).
//! * [`propagation`] – Monte Carlo propagation with deterministic parallel
//!   evaluation and summary statistics.
//! * [`sensitivity`] – Sobol indices (Saltelli/Jansen), Morris screening,
//!   finite-difference local sensitivities.
//! * [`information`] – expected value of information and the
//!   [`information::FidelitySelector`] that produces a measurable
//!   `simorph_core::Tradeoff`.
//! * [`surrogate`] – quadratic response-surface fit with leave-one-out error.
//!
//! All stochastic routines take an explicit seed and are bit-reproducible
//! regardless of the number of threads.

pub mod information;
pub mod propagation;
pub mod rng;
pub mod sampling;
pub mod sensitivity;
pub mod stats;
pub mod surrogate;

pub use information::*;
pub use propagation::*;
pub use rng::*;
pub use sampling::*;
pub use sensitivity::*;
pub use stats::*;
pub use surrogate::*;
