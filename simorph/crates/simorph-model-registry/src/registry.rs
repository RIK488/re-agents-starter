//! Registry, discovery, compatibility and chain planning.

use crate::descriptor::{Formalism, ModelDescriptor};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{Capability, ComponentId, ComponentManifest, ModelId, Result, SimorphError};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

/// One step in a component chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainStep {
    pub component: ComponentId,
    pub name: String,
    /// Outputs of this step that are consumed later or required.
    pub provides: Vec<String>,
}

/// An ordered list of components that produces the required outputs from
/// the available inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainPlan {
    pub steps: Vec<ChainStep>,
    /// Outputs that no registered component can produce.
    pub unsatisfied: Vec<String>,
}

impl ChainPlan {
    pub fn complete(&self) -> bool {
        self.unsatisfied.is_empty()
    }
}

/// Persisted form of the registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RegistryFile {
    manifests: Vec<ComponentManifest>,
    models: Vec<ModelDescriptor>,
}

/// Registry of component manifests and model descriptors.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    manifests: IndexMap<ComponentId, ComponentManifest>,
    models: IndexMap<ModelId, ModelDescriptor>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a manifest after structural validation and unit parsing of
    /// every port.
    /// A manifest with the same id or the same `name@version` as a
    /// registered one is refused (audit: it silently replaced it); use
    /// [`Self::replace_manifest`] to replace on purpose.
    pub fn register_manifest(&mut self, m: ComponentManifest) -> Result<ComponentId> {
        if let Some(old) = self
            .manifests
            .values()
            .find(|o| o.id == m.id || (o.name == m.name && o.version == m.version))
        {
            return Err(SimorphError::InvalidInput(format!(
                "component `{}@{}` is already registered (id {}); use replace_manifest",
                old.name, old.version, old.id
            )));
        }
        self.insert_manifest(m)
    }

    /// Register a manifest, replacing any registered manifest with the same
    /// id or `name@version`.  Returns the id.
    pub fn replace_manifest(&mut self, m: ComponentManifest) -> Result<ComponentId> {
        m.validate_structure()?;
        let stale: Vec<ComponentId> = self
            .manifests
            .values()
            .filter(|o| o.id == m.id || (o.name == m.name && o.version == m.version))
            .map(|o| o.id)
            .collect();
        for id in stale {
            self.manifests.shift_remove(&id);
        }
        self.insert_manifest(m)
    }

    fn insert_manifest(&mut self, m: ComponentManifest) -> Result<ComponentId> {
        m.validate_structure()?;
        for p in m.input_schema.iter().chain(m.output_schema.iter()) {
            simorph_units::parse_unit(&p.unit).map_err(|e| {
                SimorphError::Unit(format!("component `{}` port `{}`: {e}", m.name, p.name))
            })?;
        }
        let id = m.id;
        self.manifests.insert(id, m);
        Ok(id)
    }

    /// Register a model descriptor.  Refused: duplicate id or
    /// `name@version` (use [`Self::replace_model`]), missing dependencies,
    /// dimensionally inconsistent equations (equations that cannot be
    /// checked are accepted — see [`ModelDescriptor::equation_dimension_reports`]).
    pub fn register_model(&mut self, d: ModelDescriptor) -> Result<ModelId> {
        if let Some(old) = self
            .models
            .values()
            .find(|o| o.id == d.id || (o.name == d.name && o.version == d.version))
        {
            return Err(SimorphError::InvalidInput(format!(
                "model `{}@{}` is already registered; use replace_model",
                old.name, old.version
            )));
        }
        self.insert_model(d)
    }

    /// Register a model, replacing one with the same id or `name@version`.
    pub fn replace_model(&mut self, d: ModelDescriptor) -> Result<ModelId> {
        let stale: Vec<ModelId> = self
            .models
            .values()
            .filter(|o| o.id == d.id || (o.name == d.name && o.version == d.version))
            .map(|o| o.id)
            .collect();
        let backup: Vec<(ModelId, ModelDescriptor)> =
            stale.iter().map(|id| (*id, self.models[id].clone())).collect();
        for id in &stale {
            self.models.shift_remove(id);
        }
        match self.insert_model(d) {
            Ok(id) => Ok(id),
            Err(e) => {
                for (id, m) in backup {
                    self.models.insert(id, m);
                }
                Err(e)
            }
        }
    }

    fn insert_model(&mut self, d: ModelDescriptor) -> Result<ModelId> {
        d.validate_structure()?;
        for (name, check) in d.equation_dimension_reports() {
            if let crate::descriptor::EquationCheck::Inconsistent(why) = check {
                return Err(SimorphError::Unit(format!(
                    "model `{}` equation `{name}` is dimensionally inconsistent: {why}",
                    d.name
                )));
            }
        }
        for dep in &d.dependencies {
            if !self.models.contains_key(dep) {
                return Err(SimorphError::NotFound(format!(
                    "model dependency {dep} of `{}`",
                    d.name
                )));
            }
        }
        let id = d.id;
        self.models.insert(id, d);
        Ok(id)
    }

    pub fn manifest(&self, id: ComponentId) -> Option<&ComponentManifest> {
        self.manifests.get(&id)
    }
    pub fn model(&self, id: ModelId) -> Option<&ModelDescriptor> {
        self.models.get(&id)
    }
    pub fn manifests(&self) -> impl Iterator<Item = &ComponentManifest> {
        self.manifests.values()
    }
    pub fn models(&self) -> impl Iterator<Item = &ModelDescriptor> {
        self.models.values()
    }
    pub fn find_manifest_by_name(&self, name: &str) -> Option<&ComponentManifest> {
        self.manifests.values().find(|m| m.name == name)
    }

    // ---- discovery -------------------------------------------------------

    pub fn by_capability(&self, cap: &Capability) -> Vec<&ComponentManifest> {
        self.manifests
            .values()
            .filter(|m| m.has_capability(cap))
            .collect()
    }

    pub fn by_domain(&self, domain: &str) -> Vec<&ComponentManifest> {
        let d = domain.to_lowercase();
        self.manifests
            .values()
            .filter(|m| m.supported_domains.iter().any(|x| x.to_lowercase() == d))
            .collect()
    }

    pub fn models_by_formalism(&self, f: &Formalism) -> Vec<&ModelDescriptor> {
        self.models.values().filter(|m| &m.formalism == f).collect()
    }

    pub fn models_by_domain(&self, domain: &str) -> Vec<&ModelDescriptor> {
        let d = domain.to_lowercase();
        self.models
            .values()
            .filter(|m| m.domains.iter().any(|x| x.to_lowercase() == d))
            .collect()
    }

    /// Components that can produce a given output port name.
    pub fn producers_of(&self, output: &str) -> Vec<&ComponentManifest> {
        self.manifests
            .values()
            .filter(|m| m.output_schema.iter().any(|p| p.name == output))
            .collect()
    }

    // ---- compatibility ---------------------------------------------------

    /// Check that `producer.out_port` can feed `consumer.in_port`:
    /// both ports exist, same dimension, compatible shapes.
    pub fn check_compatibility(
        producer: &ComponentManifest,
        out_port: &str,
        consumer: &ComponentManifest,
        in_port: &str,
    ) -> Result<()> {
        let out = producer
            .output_schema
            .iter()
            .find(|p| p.name == out_port)
            .ok_or_else(|| {
                SimorphError::NotFound(format!("output `{out_port}` on `{}`", producer.name))
            })?;
        let inp = consumer
            .input_schema
            .iter()
            .find(|p| p.name == in_port)
            .ok_or_else(|| {
                SimorphError::NotFound(format!("input `{in_port}` on `{}`", consumer.name))
            })?;
        simorph_units::check_same_dimension(&out.unit, &inp.unit).map_err(|e| {
            SimorphError::Incompatible(format!(
                "`{}`.{out_port} -> `{}`.{in_port}: {e}",
                producer.name, consumer.name
            ))
        })?;
        if out.shape != inp.shape {
            return Err(SimorphError::Incompatible(format!(
                "`{}`.{out_port} shape {:?} != `{}`.{in_port} shape {:?}",
                producer.name, out.shape, consumer.name, inp.shape
            )));
        }
        Ok(())
    }

    // ---- chain planning --------------------------------------------------

    /// Find an ordered list of registered components producing every
    /// `required_outputs` name starting from `available_inputs` names.
    /// Greedy forward chaining: repeatedly add any component whose inputs
    /// are all available; stop when the requirements are met or nothing new
    /// can be produced.  Returns the minimal subset (components that do not
    /// contribute to a required output are pruned).
    ///
    /// A component input is satisfied by an earlier component output only if
    /// the names match **and** the units have the same dimension and the
    /// shapes agree (audit: a `T` in kelvin used to feed a `T` in metres).
    /// Inputs given by name in `available_inputs` carry no unit and match by
    /// name; use [`Self::plan_chain_typed`] to declare their units.
    pub fn plan_chain(&self, required_outputs: &[&str], available_inputs: &[&str]) -> ChainPlan {
        let typed: Vec<(&str, Option<&str>)> = available_inputs.iter().map(|n| (*n, None)).collect();
        self.plan_chain_inner(required_outputs, &typed)
    }

    /// [`Self::plan_chain`] with units for the available inputs
    /// (`(name, unit)`), which are then also matched by dimension.
    pub fn plan_chain_typed(&self, required_outputs: &[&str], available_inputs: &[(&str, &str)]) -> ChainPlan {
        let typed: Vec<(&str, Option<&str>)> =
            available_inputs.iter().map(|(n, u)| (*n, Some(*u))).collect();
        self.plan_chain_inner(required_outputs, &typed)
    }

    fn plan_chain_inner(&self, required_outputs: &[&str], typed_inputs: &[(&str, Option<&str>)]) -> ChainPlan {
        let available_inputs: Vec<&str> = typed_inputs.iter().map(|(n, _)| *n).collect();
        let available_inputs = available_inputs.as_slice();
        // name -> (unit, shape) of what provides it; None = untyped user input
        let mut typed: HashMap<String, Option<(String, Vec<usize>)>> = typed_inputs
            .iter()
            .map(|(n, u)| (n.to_string(), u.map(|u| (u.to_string(), vec![]))))
            .collect();
        let fits = |typed: &HashMap<String, Option<(String, Vec<usize>)>>, p: &simorph_core::Port| -> bool {
            match typed.get(&p.name) {
                None => false,
                Some(None) => true,
                Some(Some((unit, shape))) => {
                    simorph_units::check_same_dimension(unit, &p.unit).is_ok()
                        && (shape == &p.shape || shape.is_empty() && typed_inputs.iter().any(|(n, _)| *n == p.name))
                }
            }
        };
        let mut available: HashSet<String> =
            available_inputs.iter().map(|s| s.to_string()).collect();
        let required: BTreeSet<String> = required_outputs.iter().map(|s| s.to_string()).collect();
        let mut order: Vec<ComponentId> = vec![];
        let mut used: HashSet<ComponentId> = HashSet::new();
        loop {
            if required.iter().all(|r| available.contains(r)) {
                break;
            }
            let mut progressed = false;
            for m in self.manifests.values() {
                if used.contains(&m.id) {
                    continue;
                }
                let inputs_ok = m.input_schema.iter().all(|p| fits(&typed, p));
                let adds_new = m.output_schema.iter().any(|p| !available.contains(&p.name));
                if inputs_ok && adds_new {
                    used.insert(m.id);
                    order.push(m.id);
                    for p in &m.output_schema {
                        if available.insert(p.name.clone()) {
                            typed.insert(p.name.clone(), Some((p.unit.clone(), p.shape.clone())));
                        }
                    }
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        // Prune: walk backwards keeping only components that provide a
        // needed name.
        let mut needed: HashSet<String> = required.iter().cloned().collect();
        let mut kept: Vec<ChainStep> = vec![];
        for id in order.iter().rev() {
            let m = &self.manifests[id];
            let provides: Vec<String> = m
                .output_schema
                .iter()
                .filter(|p| needed.contains(&p.name))
                .map(|p| p.name.clone())
                .collect();
            if provides.is_empty() {
                continue;
            }
            for p in &provides {
                needed.remove(p);
            }
            for p in &m.input_schema {
                if !available_inputs.contains(&p.name.as_str()) {
                    needed.insert(p.name.clone());
                }
            }
            kept.push(ChainStep {
                component: m.id,
                name: m.name.clone(),
                provides,
            });
        }
        kept.reverse();
        let unsatisfied: Vec<String> = required
            .into_iter()
            .filter(|r| !available.contains(r))
            .collect();
        ChainPlan {
            steps: kept,
            unsatisfied,
        }
    }

    /// Topological order of registered models by dependency.
    pub fn model_dependency_order(&self) -> Result<Vec<ModelId>> {
        // audit: a dependency listed twice counted twice in the in-degree but
        // was released once, so it was reported as a cycle
        let deps = |m: &ModelDescriptor| -> BTreeSet<ModelId> {
            m.dependencies.iter().copied().filter(|d| self.models.contains_key(d)).collect()
        };
        let mut indeg: HashMap<ModelId, usize> = self.models.keys().map(|k| (*k, 0)).collect();
        for m in self.models.values() {
            *indeg.get_mut(&m.id).unwrap() = deps(m).len();
        }
        let mut queue: VecDeque<ModelId> = self
            .models
            .keys()
            .filter(|k| indeg[k] == 0)
            .copied()
            .collect();
        let mut out = vec![];
        while let Some(id) = queue.pop_front() {
            out.push(id);
            for m in self.models.values() {
                if m.dependencies.contains(&id) {
                    let e = indeg.get_mut(&m.id).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        queue.push_back(m.id);
                    }
                }
            }
        }
        if out.len() != self.models.len() {
            return Err(SimorphError::InvalidInput(
                "cyclic model dependencies".into(),
            ));
        }
        Ok(out)
    }

    // ---- persistence -----------------------------------------------------

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let f = RegistryFile {
            manifests: self.manifests.values().cloned().collect(),
            models: self.models.values().cloned().collect(),
        };
        std::fs::write(path, serde_json::to_string_pretty(&f)?)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let f: RegistryFile = serde_json::from_str(&text)?;
        let mut r = Registry::new();
        for m in f.manifests {
            r.register_manifest(m)?;
        }
        // models may reference each other: insert in dependency-respecting order
        let mut pending = f.models;
        let mut guard = 0;
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = vec![];
            for d in pending {
                if d.dependencies.iter().all(|dep| r.models.contains_key(dep)) {
                    r.register_model(d)?;
                } else {
                    rest.push(d);
                }
            }
            pending = rest;
            guard += 1;
            if pending.len() == before || guard > 10_000 {
                return Err(SimorphError::InvalidInput(
                    "unresolvable model dependencies in registry file".into(),
                ));
            }
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::Port;

    fn orbit() -> ComponentManifest {
        ComponentManifest::in_process("orbit", "1", "t")
            .with_capabilities(vec![Capability::OdeIntegration])
            .with_domains(&["aerospace"])
            .with_inputs(vec![Port::scalar("altitude_km", "km")])
            .with_outputs(vec![
                Port::scalar("eclipse", "1"),
                Port::scalar("irradiance", "W/m^2"),
            ])
    }
    fn solar() -> ComponentManifest {
        ComponentManifest::in_process("solar", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["power"])
            .with_inputs(vec![
                Port::scalar("irradiance", "kW/m^2"),
                Port::scalar("area", "m^2"),
            ])
            .with_outputs(vec![Port::scalar("power", "W")])
    }
    fn battery() -> ComponentManifest {
        ComponentManifest::in_process("battery", "1", "t")
            .with_capabilities(vec![Capability::OdeIntegration])
            .with_domains(&["power"])
            .with_inputs(vec![Port::scalar("power", "W")])
            .with_outputs(vec![Port::scalar("soc", "1")])
    }
    fn unrelated() -> ComponentManifest {
        ComponentManifest::in_process("thermal", "1", "t")
            .with_domains(&["thermal"])
            .with_inputs(vec![Port::scalar("heat", "W")])
            .with_outputs(vec![Port::scalar("temperature", "K")])
    }

    #[test]
    fn discovery_filters() {
        let mut r = Registry::new();
        r.register_manifest(orbit()).unwrap();
        r.register_manifest(solar()).unwrap();
        r.register_manifest(battery()).unwrap();
        assert_eq!(r.by_capability(&Capability::OdeIntegration).len(), 2);
        assert_eq!(r.by_domain("Power").len(), 2);
        assert_eq!(r.producers_of("power").len(), 1);
        let d = ModelDescriptor::new("thermal-node", "1", Formalism::Ode)
            .with_domains(&["thermal"])
            .with_outputs(vec![Port::scalar("T", "K")]);
        r.register_model(d).unwrap();
        assert_eq!(r.models_by_formalism(&Formalism::Ode).len(), 1);
        assert_eq!(r.models_by_domain("thermal").len(), 1);
    }

    #[test]
    fn dimensional_mismatch_rejected_and_prefix_accepted() {
        assert!(
            Registry::check_compatibility(&orbit(), "irradiance", &solar(), "irradiance").is_ok()
        );
        let err =
            Registry::check_compatibility(&orbit(), "eclipse", &solar(), "irradiance").unwrap_err();
        assert!(matches!(err, SimorphError::Incompatible(_)));
        assert!(Registry::check_compatibility(&orbit(), "nope", &solar(), "irradiance").is_err());
    }

    #[test]
    fn bad_unit_in_manifest_is_rejected_at_registration() {
        let mut r = Registry::new();
        let m = ComponentManifest::in_process("bad", "1", "t")
            .with_inputs(vec![Port::scalar("x", "furlongs")]);
        assert!(r.register_manifest(m).is_err());
    }

    #[test]
    fn chain_planning_finds_two_step_chain_and_prunes() {
        let mut r = Registry::new();
        r.register_manifest(unrelated()).unwrap();
        r.register_manifest(orbit()).unwrap();
        r.register_manifest(solar()).unwrap();
        r.register_manifest(battery()).unwrap();
        let plan = r.plan_chain(&["soc"], &["altitude_km", "area"]);
        assert!(plan.complete());
        let names: Vec<&str> = plan.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["orbit", "solar", "battery"]);
        let partial = r.plan_chain(&["soc", "mass"], &["altitude_km", "area"]);
        assert_eq!(partial.unsatisfied, vec!["mass".to_string()]);
        let none = r.plan_chain(&["soc"], &[]);
        assert!(!none.complete());
    }

    #[test]
    fn model_dependencies_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Registry::new();
        r.register_manifest(orbit()).unwrap();
        let base = ModelDescriptor::new("base", "1", Formalism::Algebraic)
            .with_outputs(vec![Port::scalar("y", "m")]);
        let base_id = r.register_model(base).unwrap();
        let derived = ModelDescriptor::new("derived", "1", Formalism::Surrogate)
            .with_dependency(base_id)
            .with_inputs(vec![Port::scalar("y", "m")])
            .with_equation("fit", "z = a*y + b");
        let derived_id = r.register_model(derived).unwrap();
        assert_eq!(
            r.model_dependency_order().unwrap(),
            vec![base_id, derived_id]
        );
        let missing =
            ModelDescriptor::new("orphan", "1", Formalism::Ode).with_dependency(ModelId::new());
        assert!(r.register_model(missing).is_err());
        let p = dir.path().join("registry.json");
        r.save(&p).unwrap();
        let back = Registry::load(&p).unwrap();
        assert_eq!(back.manifests.len(), 1);
        assert_eq!(back.models.len(), 2);
        assert_eq!(
            back.model(derived_id).unwrap().equations[0].expression,
            "z = a*y + b"
        );
    }
}
