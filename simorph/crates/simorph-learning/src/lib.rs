//! # simorph-learning
//!
//! Native machine learning used by SIMORPH's AI laboratory and closed-loop
//! improvement service.  Everything here is deterministic for a given seed
//! (ChaCha RNG, no thread-level non-determinism), CPU-only and dependency-free
//! apart from `rand`.
//!
//! * [`mlp`] – dense multilayer perceptron: initialisation, forward pass,
//!   backpropagation, MSE / softmax cross-entropy, Adam and SGD-momentum,
//!   mini-batches, early stopping, standardisation, JSON serialisation with a
//!   content hash.
//! * [`surrogate`] – a trained regressor exposed as a `simorph_core::Component`.
//! * [`rl`] – environments (`CartPole`, `Pendulum`, same dynamics as
//!   Gymnasium `CartPole-v1` / `Pendulum-v1`) and algorithms (REINFORCE with
//!   baseline, PPO with GAE, discrete and Gaussian policies).
//! * [`torch`] – optional PyTorch trainer (subprocess); returns weights that
//!   load into the native [`mlp::Mlp`] for evaluation.

pub mod mlp;
pub mod rl;
pub mod surrogate;
pub mod torch;

pub use mlp::*;
pub use surrogate::SurrogateComponent;
