//! # simorph-core
//!
//! Foundational contracts shared by every SIMORPH subsystem.
//!
//! This crate deliberately contains **no domain knowledge**: no physics, no
//! biology, no economics.  It defines the vocabulary that lets domain plugins,
//! simulators, optimisers and validators interoperate:
//!
//! * [`ids`] – strongly typed, globally unique identifiers.
//! * [`evidence`] – the non-interchangeable epistemic categories required by
//!   the SIMORPH specification (computed / simulated / empirically validated /
//!   mathematically proven / unverified hypothesis).
//! * [`manifest`] – the universal `ComponentManifest` contract (§7).
//! * [`problem`] – the problem specification received by the scientific
//!   director (§2, §3.2).
//! * [`quantity`] – values carrying a unit string and an uncertainty model.
//! * [`component`] – the universal execution interface (discover / check
//!   compatibility / prepare / submit / control / collect / cancel / reproduce /
//!   validate / publish metrics).
//! * [`tradeoff`] – the measurable precision/time/cost/risk/information
//!   trade-off used by uncertainty-guided simulation (§8).
//! * [`error`] – the shared error type.

pub mod component;
pub mod error;
pub mod evidence;
pub mod hashing;
pub mod ids;
pub mod manifest;
pub mod problem;
pub mod quantity;
pub mod tradeoff;

pub use component::*;
pub use error::{Result, SimorphError};
pub use evidence::*;
pub use ids::*;
pub use manifest::*;
pub use problem::*;
pub use quantity::*;
pub use tradeoff::*;
