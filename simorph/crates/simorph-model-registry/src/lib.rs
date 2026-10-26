//! # simorph-model-registry
//!
//! Universal model representation (§3.3) and component discovery (§7).
//!
//! * [`ModelDescriptor`] – declarative description of a model: formalism,
//!   variables with units, parameters, equations, assumptions, validity
//!   domain, dependencies, uncertainty, verification criteria.
//! * [`Registry`] – stores manifests and descriptors, discovers them by
//!   capability / domain / formalism, checks port compatibility
//!   dimensionally and plans component chains.
//! * [`export_schemas`] – writes the JSON Schemas of the public contracts.

pub mod descriptor;
pub mod registry;
pub mod schemas;

pub use descriptor::{Equation, Formalism, ModelDescriptor, VerificationCriterion};
pub use registry::{ChainPlan, ChainStep, Registry};
pub use schemas::export_schemas;
