//! # simorph-plugin-physics
//!
//! Domain models exposed through the universal component contract and the
//! co-simulation slave contract.  Every model declares units, assumptions,
//! validity domain, uncertainty model and reference cases in its manifest,
//! and every model is verified against an analytic solution or a
//! conservation law in this crate's tests.
//!
//! | module       | model                                   | kind      |
//! |--------------|-----------------------------------------|-----------|
//! | [`orbit`]    | two-body + optional J2, cylindrical eclipse | ODE   |
//! | [`thermal`]  | lumped-parameter thermal network         | ODE       |
//! | [`battery`]  | equivalent-circuit battery (SOC, OCV, R) | ODE       |
//! | [`solar`]    | solar array power (cosine law, thermal derating) | algebraic |
//! | [`attitude`] | rigid-body attitude (Euler + quaternion, PD control) | ODE |
//!
//! Discovery: [`manifests`], [`build`], [`slave`].

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod attitude;
pub mod battery;
pub mod demo;
pub mod orbit;
pub mod solar;
pub mod thermal;

use simorph_core::{Component, ComponentId, ComponentManifest, Result};
use simorph_cosimulation::CosimSlave;
use simorph_simulation::{Solver, SolverOptions};

pub const VERSION: &str = "0.1.0";
pub const PROVIDER: &str = "simorph-plugin-physics";

/// Default solver settings used when models are exposed as slaves.
pub fn default_solver() -> SolverOptions {
    SolverOptions::new(Solver::Rk45).with_tolerances(1e-8, 1e-10)
}

/// All component manifests offered by this plugin.
pub fn manifests() -> Vec<ComponentManifest> {
    components()
        .into_iter()
        .map(|c| c.manifest().clone())
        .collect()
}

/// All components offered by this plugin (fresh instances).
pub fn components() -> Vec<Box<dyn Component>> {
    vec![
        orbit::component(),
        thermal::component(2),
        battery::component(),
        solar::component(),
        attitude::component(),
    ]
}

/// Build a component by id (see [`manifests`]).
pub fn build(id: ComponentId) -> Option<Box<dyn Component>> {
    components().into_iter().find(|c| c.manifest().id == id)
}

/// Build a co-simulation slave by model name
/// (`"physics.orbit.two_body"`, `"physics.thermal.network"` (2 nodes),
/// `"physics.battery.equivalent_circuit"`, `"physics.solar.array"`,
/// `"physics.attitude.rigid_body"`).
pub fn slave(name: &str) -> Result<Box<dyn CosimSlave>> {
    Ok(match name {
        orbit::NAME => Box::new(orbit::slave()?),
        thermal::NAME => Box::new(thermal::slave(2)?),
        battery::NAME => Box::new(battery::slave()?),
        solar::NAME => Box::new(solar::slave()),
        attitude::NAME => Box::new(attitude::slave()?),
        other => {
            return Err(simorph_core::SimorphError::NotFound(format!(
                "physics model `{other}`"
            )))
        }
    })
}
