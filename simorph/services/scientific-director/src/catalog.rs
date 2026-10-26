//! Read-only view of the component registry used by the director.
//!
//! The director never instantiates components itself; it only reasons about
//! manifests.  The concrete implementation over `simorph-model-registry` lives
//! in the orchestrating application so that this crate stays independent.

use simorph_core::{Capability, ComponentId, ComponentManifest};

/// Catalogue of available component manifests.
pub trait ComponentCatalog: Send + Sync {
    fn manifests(&self) -> Vec<ComponentManifest>;

    /// Components producing an output port with this name.
    fn producers_of(&self, output: &str) -> Vec<ComponentManifest> {
        self.manifests()
            .into_iter()
            .filter(|m| m.output_schema.iter().any(|p| p.name == output))
            .collect()
    }

    fn get(&self, id: ComponentId) -> Option<ComponentManifest> {
        self.manifests().into_iter().find(|m| m.id == id)
    }

    /// Whether the catalogue holds *empirical* reference cases for a component
    /// (used to decide whether a simulated quantity still needs experimental
    /// verification).  Default: look for reference cases whose source mentions
    /// measurement / experiment / observation.
    fn has_empirical_reference(&self, id: ComponentId) -> bool {
        self.get(id)
            .map(|m| {
                m.validation_protocol.reference_cases.iter().any(|r| {
                    let s = r.source.to_lowercase();
                    s.contains("measure")
                        || s.contains("experiment")
                        || s.contains("observ")
                        || s.contains("mesure")
                        || s.contains("expéri")
                })
            })
            .unwrap_or(false)
    }
}

/// In-memory catalogue (tests, CLI).
#[derive(Default, Clone)]
pub struct StaticCatalog {
    pub items: Vec<ComponentManifest>,
}

impl StaticCatalog {
    pub fn new(items: Vec<ComponentManifest>) -> Self {
        Self { items }
    }
}

impl ComponentCatalog for StaticCatalog {
    fn manifests(&self) -> Vec<ComponentManifest> {
        self.items.clone()
    }
}

/// Classification of a component's computational nature from its capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeKind {
    /// Closed-form / exact: result is `Computed`.
    Direct,
    /// Time-stepping or stochastic simulation: result is `Simulated`.
    Simulation,
    /// Learning / inference: data-driven.
    DataDriven,
    /// Formal verification.
    Proof,
    Other,
}

pub fn compute_kind(m: &ComponentManifest) -> ComputeKind {
    use Capability::*;
    let caps = &m.capabilities;
    if caps.iter().any(|c| matches!(c, FormalVerification)) {
        ComputeKind::Proof
    } else if caps.iter().any(|c| {
        matches!(
            c,
            OdeIntegration
                | DaeIntegration
                | PdeSolver
                | DiscreteEvent
                | MultiAgent
                | PhysicsEngine
                | CosimulationSlave
                | ProbabilisticInference
        )
    }) {
        ComputeKind::Simulation
    } else if caps.iter().any(|c| matches!(c, Training | Inference)) {
        ComputeKind::DataDriven
    } else if caps.iter().any(|c| {
        matches!(
            c,
            AlgebraicSolver
                | LinearAlgebra
                | SymbolicMath
                | Optimisation
                | UncertaintyQuantification
        )
    }) {
        ComputeKind::Direct
    } else {
        ComputeKind::Other
    }
}
