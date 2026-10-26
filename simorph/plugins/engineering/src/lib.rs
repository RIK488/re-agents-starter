//! # simorph-plugin-engineering
//!
//! Algebraic engineering, economic and operations models that demonstrate the
//! universality of the component contract outside physics.  Every model is a
//! `simorph_core::Component` with a complete manifest (units, assumptions,
//! validity domain, reference cases from closed-form solutions).
//!
//! Modules:
//! * [`beam`] – Euler–Bernoulli cantilever (civil / mechanical engineering).
//! * [`pipe_flow`] – Darcy–Weisbach pressure drop with Colebrook friction (energy, process, mining).
//! * [`finance`] – net present value and internal rate of return (economics / finance).
//! * [`queue`] – M/M/1 queue steady-state metrics (logistics, telecoms, computing).
//! * [`wind`] – wind turbine power with Betz limit (energy).
//! * [`growth`] – logistic growth closed form (biology, agriculture, epidemiology baseline).

use simorph_core::{Component, ComponentId, ComponentManifest};

pub mod beam;
pub mod finance;
pub mod growth;
pub mod pipe_flow;
pub mod queue;
pub mod wind;

/// All manifests provided by this plugin (for registry discovery).
pub fn manifests() -> Vec<ComponentManifest> {
    vec![
        beam::Cantilever::new().manifest().clone(),
        pipe_flow::PipeFlow::new().manifest().clone(),
        finance::NetPresentValue::new().manifest().clone(),
        queue::Mm1Queue::new().manifest().clone(),
        wind::WindTurbine::new().manifest().clone(),
        growth::LogisticGrowth::new().manifest().clone(),
    ]
}

/// Build a component by id.
pub fn build(id: ComponentId) -> Option<Box<dyn Component>> {
    let all: Vec<Box<dyn Component>> = vec![
        Box::new(beam::Cantilever::new()),
        Box::new(pipe_flow::PipeFlow::new()),
        Box::new(finance::NetPresentValue::new()),
        Box::new(queue::Mm1Queue::new()),
        Box::new(wind::WindTurbine::new()),
        Box::new(growth::LogisticGrowth::new()),
    ];
    all.into_iter().find(|c| c.manifest().id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_manifest_is_structurally_valid_and_buildable() {
        for m in manifests() {
            m.validate_structure().unwrap();
            assert!(build(m.id).is_some(), "{} not buildable", m.name);
        }
    }
}
