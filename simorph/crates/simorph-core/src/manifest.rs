//! The universal component contract (§7 of the specification).
//!
//! Every scientific engine, simulator, solver, optimiser or external tool that
//! SIMORPH can call must expose a [`ComponentManifest`].  Tools that cannot
//! produce one natively are wrapped by an adapter that produces it on their
//! behalf.

use crate::ids::ComponentId;
use crate::quantity::Port;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Kind of computational capability a component offers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Time-domain integration of ordinary differential equations.
    OdeIntegration,
    /// Differential-algebraic systems.
    DaeIntegration,
    /// Partial differential equations (field problems).
    PdeSolver,
    /// Algebraic / root-finding systems.
    AlgebraicSolver,
    /// Linear algebra kernels.
    LinearAlgebra,
    /// Discrete-event simulation.
    DiscreteEvent,
    /// Agent-based / multi-agent simulation.
    MultiAgent,
    /// Rigid or soft body dynamics.
    PhysicsEngine,
    /// Sampling from probabilistic models.
    ProbabilisticInference,
    /// Optimisation (see `OptimisationFamily` in `simorph-optimization`).
    Optimisation,
    /// Uncertainty quantification / sensitivity analysis.
    UncertaintyQuantification,
    /// Symbolic mathematics.
    SymbolicMath,
    /// Formal proof / SMT.
    FormalVerification,
    /// Model training (machine learning).
    Training,
    /// Model evaluation / benchmarking.
    Evaluation,
    /// Inference serving.
    Inference,
    /// Data storage and retrieval.
    DataStore,
    /// Visualisation / rendering.
    Visualisation,
    /// Co-simulation slave (FMI-like step/exchange interface).
    CosimulationSlave,
    /// Anything not covered above.  The string is a free-form tag.
    Other(String),
}

/// Declared numerical precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NumericalPrecision {
    Float16,
    Float32,
    Float64,
    Float128,
    ArbitraryPrecision,
    Exact,
    Mixed,
}

/// Whether and how the component can be made bit-for-bit deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicMode {
    /// Always deterministic for a given input and seed.
    Always,
    /// Deterministic only when a seed is supplied.
    WithSeed,
    /// Deterministic only with a specific configuration (described).
    Conditional { description: String },
    /// Cannot be made deterministic (e.g. hardware non-determinism).
    Never,
}

/// Hardware / software resources needed to run a component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ResourceRequirements {
    #[serde(default)]
    pub cpu_cores: f64,
    #[serde(default)]
    pub memory_mb: u64,
    #[serde(default)]
    pub gpu_count: u32,
    #[serde(default)]
    pub gpu_memory_mb: u64,
    /// Expected wall-clock upper bound per task, in seconds (0 = unknown).
    #[serde(default)]
    pub wall_time_s: f64,
    /// Whether network egress is required.
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub disk_mb: u64,
}

/// How the component is invoked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionInterface {
    /// Native Rust implementation registered in-process.
    InProcess,
    /// Child process speaking the SIMORPH JSON-lines protocol over stdio.
    Subprocess { command: String, #[serde(default)] args: Vec<String> },
    /// HTTP/JSON endpoint.
    Http { base_url: String },
    /// gRPC endpoint.
    Grpc { endpoint: String },
    /// Python module loaded through the python bridge.
    Python { module: String, #[serde(default)] entrypoint: String },
    /// FMI Functional Mock-up Unit.
    Fmu { path: String, fmi_version: String },
    /// WebAssembly component.
    Wasm { path: String },
    /// Containerised component.
    Container { image: String, #[serde(default)] command: Vec<String> },
}

/// Validation protocol declared by the component: how its outputs can be
/// checked (reference cases, analytic solutions, tolerances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ValidationProtocol {
    /// Named reference cases with expected outputs.
    #[serde(default)]
    pub reference_cases: Vec<ReferenceCase>,
    /// Invariants that must hold on every output (free-form expressions
    /// evaluated by the validation engine, e.g. `"energy_drift < 1e-6"`).
    #[serde(default)]
    pub invariants: Vec<String>,
    /// Expected order of convergence for discretisation parameters, if any.
    #[serde(default)]
    pub convergence_order: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceCase {
    pub name: String,
    pub inputs: serde_json::Value,
    pub expected_outputs: serde_json::Value,
    /// Relative tolerance.
    pub rtol: f64,
    /// Absolute tolerance.
    pub atol: f64,
    /// Where the reference comes from (analytic formula, paper, measurement).
    pub source: String,
}

/// Provenance of the component itself (not of its outputs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ComponentProvenance {
    #[serde(default)]
    pub source_repository: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub build_hash: Option<String>,
    #[serde(default)]
    pub generated_by: Option<String>,
    #[serde(default)]
    pub citation: Option<String>,
}

/// Security permissions a component requests.  The security crate decides
/// whether they are granted; components never grant themselves anything.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadWorkspace,
    WriteWorkspace,
    NetworkEgress,
    SpawnProcess,
    Gpu,
    ReadSecrets,
    /// Any action affecting the physical world or external services with
    /// side effects (actuators, purchases, deployments).  Never granted
    /// automatically to autonomous agents.
    ExternalSideEffect,
    /// Modify SIMORPH's own code or configuration.
    SelfModify,
}

/// Declared validity domain: per-parameter ranges and textual conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ValidityDomain {
    /// Parameter name -> (min, max) in the parameter's declared unit.
    #[serde(default)]
    pub ranges: IndexMap<String, (f64, f64)>,
    /// Conditions that cannot be expressed as ranges.
    #[serde(default)]
    pub conditions: Vec<String>,
}

/// Declared uncertainty model of the component outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UncertaintyModel {
    /// Component does not quantify its own uncertainty.
    None,
    /// Component reports a global relative error bound.
    RelativeErrorBound { rtol: f64 },
    /// Component reports per-output standard deviations.
    PerOutputStdDev,
    /// Component returns samples / ensembles.
    Ensemble,
    /// Component returns an interval per output.
    Interval,
    /// Free text description.
    Described { description: String },
}

/// The universal manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentManifest {
    pub id: ComponentId,
    pub name: String,
    pub version: String,
    pub provider: String,
    /// SPDX license identifier or free text.
    pub license: String,
    pub capabilities: Vec<Capability>,
    /// Domain tags, e.g. `"aerospace"`, `"thermal"`, `"ml"`.  Free-form by
    /// design: no domain is hard-coded in the core.
    pub supported_domains: Vec<String>,
    pub input_schema: Vec<Port>,
    pub output_schema: Vec<Port>,
    /// Unit expressions used by the component, keyed by port name.  Allows
    /// the validation engine to check dimensional consistency.
    #[serde(default)]
    pub units: IndexMap<String, String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub validity_domain: ValidityDomain,
    pub uncertainty_model: UncertaintyModel,
    pub numerical_precision: NumericalPrecision,
    pub deterministic_mode: DeterministicMode,
    #[serde(default)]
    pub resource_requirements: ResourceRequirements,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub execution_interface: ExecutionInterface,
    #[serde(default)]
    pub validation_protocol: ValidationProtocol,
    #[serde(default)]
    pub provenance: ComponentProvenance,
    #[serde(default)]
    pub security_permissions: Vec<Permission>,
}

impl ComponentManifest {
    /// Minimal manifest builder for in-process Rust components.
    pub fn in_process(name: &str, version: &str, provider: &str) -> Self {
        Self {
            id: ComponentId::deterministic(&format!("{name}@{version}")),
            name: name.to_string(),
            version: version.to_string(),
            provider: provider.to_string(),
            license: "Apache-2.0".into(),
            capabilities: vec![],
            supported_domains: vec![],
            input_schema: vec![],
            output_schema: vec![],
            units: IndexMap::new(),
            assumptions: vec![],
            validity_domain: ValidityDomain::default(),
            uncertainty_model: UncertaintyModel::None,
            numerical_precision: NumericalPrecision::Float64,
            deterministic_mode: DeterministicMode::Always,
            resource_requirements: ResourceRequirements::default(),
            dependencies: vec![],
            execution_interface: ExecutionInterface::InProcess,
            validation_protocol: ValidationProtocol::default(),
            provenance: ComponentProvenance::default(),
            security_permissions: vec![],
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.capabilities = caps;
        self
    }
    pub fn with_domains(mut self, domains: &[&str]) -> Self {
        self.supported_domains = domains.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn with_inputs(mut self, ports: Vec<Port>) -> Self {
        for p in &ports {
            self.units.insert(p.name.clone(), p.unit.clone());
        }
        self.input_schema = ports;
        self
    }
    pub fn with_outputs(mut self, ports: Vec<Port>) -> Self {
        for p in &ports {
            self.units.insert(p.name.clone(), p.unit.clone());
        }
        self.output_schema = ports;
        self
    }
    pub fn with_assumptions(mut self, a: &[&str]) -> Self {
        self.assumptions = a.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn with_validity_range(mut self, param: &str, low: f64, high: f64) -> Self {
        self.validity_domain.ranges.insert(param.to_string(), (low, high));
        self
    }
    pub fn with_permissions(mut self, perms: Vec<Permission>) -> Self {
        self.security_permissions = perms;
        self
    }
    pub fn with_validation(mut self, v: ValidationProtocol) -> Self {
        self.validation_protocol = v;
        self
    }

    /// Structural self-check: ports must be unique and every port needs a unit.
    pub fn validate_structure(&self) -> crate::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for p in self.input_schema.iter().chain(self.output_schema.iter()) {
            if p.unit.trim().is_empty() {
                return Err(crate::SimorphError::InvalidInput(format!(
                    "port `{}` of `{}` has no unit (use \"1\" for dimensionless)",
                    p.name, self.name
                )));
            }
            if !seen.insert(&p.name) {
                return Err(crate::SimorphError::InvalidInput(format!(
                    "duplicate port `{}` in `{}`",
                    p.name, self.name
                )));
            }
        }
        if self.name.is_empty() || self.version.is_empty() {
            return Err(crate::SimorphError::InvalidInput("manifest name/version empty".into()));
        }
        Ok(())
    }

    pub fn has_capability(&self, c: &Capability) -> bool {
        self.capabilities.contains(c)
    }

    /// Content hash of the manifest (used by provenance records).
    pub fn content_hash(&self) -> String {
        crate::hashing::hash_json(self).expect("manifest is serialisable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip_and_structure() {
        let m = ComponentManifest::in_process("test.ode", "0.1.0", "simorph")
            .with_capabilities(vec![Capability::OdeIntegration])
            .with_inputs(vec![Port::scalar("x0", "m"), Port::scalar("v0", "m/s")])
            .with_outputs(vec![Port::scalar("x", "m")]);
        m.validate_structure().unwrap();
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: ComponentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.content_hash(), back.content_hash());
    }

    #[test]
    fn duplicate_ports_are_rejected() {
        let m = ComponentManifest::in_process("dup", "1", "x")
            .with_inputs(vec![Port::scalar("a", "1"), Port::scalar("a", "1")]);
        assert!(m.validate_structure().is_err());
    }

    #[test]
    fn json_schema_is_generated() {
        let schema = schemars::schema_for!(ComponentManifest);
        let s = serde_json::to_string(&schema).unwrap();
        assert!(s.contains("security_permissions"));
    }
}
