//! # simorph-optimization
//!
//! Optimisation engine of SIMORPH (§3.7).  Several families of methods are
//! provided behind one [`ObjectiveFunction`] interface, and a
//! [`MethodSelector`] chooses the methods suited to the problem's features
//! instead of applying a single algorithm systematically.
//!
//! | Family                     | Module            |
//! |----------------------------|-------------------|
//! | Direct search              | [`nelder_mead`]   |
//! | Gradient-based (GD, L-BFGS)| [`gradient`]      |
//! | Evolutionary (DE, CMA-ES)  | [`evolutionary`]  |
//! | Simulated annealing        | [`annealing`]     |
//! | Constraint handling        | [`constraints`]   |
//! | Multi-objective (NSGA-II)  | [`nsga2`]         |
//! | Bayesian optimisation (GP) | [`bayesian`]      |
//! | Tree / beam search         | [`tree_search`]   |
//! | Robust optimisation        | [`robust`]        |
//! | Reinforcement learning     | [`rl`]            |
//! | Method selection           | [`selector`]      |
//! | Component/problem adapter  | [`adapter`]       |

pub mod adapter;
pub mod annealing;
pub mod bayesian;
pub mod benchmarks;
pub mod constraints;
pub mod evolutionary;
pub mod gradient;
pub mod nelder_mead;
pub mod nsga2;
pub mod problem;
pub mod rl;
pub mod robust;
pub mod selector;
pub mod tree_search;

pub use adapter::*;
pub use annealing::*;
pub use bayesian::*;
pub use constraints::*;
pub use evolutionary::*;
pub use gradient::*;
pub use nelder_mead::*;
pub use nsga2::*;
pub use problem::*;
pub use rl::*;
pub use robust::*;
pub use selector::*;
pub use tree_search::*;
