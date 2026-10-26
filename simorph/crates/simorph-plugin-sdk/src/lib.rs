//! # simorph-plugin-sdk
//!
//! Tooling for component authors:
//!
//! * [`ComponentBuilder`] – build a [`simorph_core::Component`] from a
//!   manifest and a closure, without writing a struct.
//! * [`ComponentSet`] – a name → factory table used for discovery by the
//!   orchestrator and the registry (`register!`-style helper
//!   [`ComponentSet::register`]).
//! * [`protocol`] – the JSON-lines message types spoken between SIMORPH and
//!   out-of-process components (Python, FMI drivers, isolated Rust
//!   components).  `simorph-execution` depends on this crate for them.
//! * [`serve_stdio`] – turn any Rust component into a subprocess server, so
//!   it can be isolated behind the sandbox policy like external tools.

pub mod protocol;

use indexmap::IndexMap;
use parking_lot::RwLock;
use simorph_core::{
    Capability, Component, ComponentId, ComponentManifest, EvidenceLevel, ExecutionControl, Port,
    Result, SimorphError, TaskRequest, TaskResult,
};
use std::io::{BufRead, Write};
use std::sync::Arc;

type ExecFn = dyn Fn(&TaskRequest, &ExecutionControl) -> Result<TaskResult> + Send + Sync;

/// A component defined by a manifest and a closure.
pub struct ClosureComponent {
    manifest: ComponentManifest,
    exec: Box<ExecFn>,
}

impl Component for ClosureComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        (self.exec)(req, ctl)
    }
}

/// Fluent builder.
pub struct ComponentBuilder {
    manifest: ComponentManifest,
    evidence: EvidenceLevel,
}

impl ComponentBuilder {
    pub fn new(name: &str, version: &str, provider: &str) -> Self {
        Self {
            manifest: ComponentManifest::in_process(name, version, provider),
            evidence: EvidenceLevel::Computed,
        }
    }
    pub fn manifest(mut self, f: impl FnOnce(ComponentManifest) -> ComponentManifest) -> Self {
        self.manifest = f(self.manifest);
        self
    }
    pub fn capability(mut self, c: Capability) -> Self {
        self.manifest.capabilities.push(c);
        self
    }
    pub fn domain(mut self, d: &str) -> Self {
        self.manifest.supported_domains.push(d.to_string());
        self
    }
    pub fn input(mut self, p: Port) -> Self {
        self.manifest.units.insert(p.name.clone(), p.unit.clone());
        self.manifest.input_schema.push(p);
        self
    }
    pub fn output(mut self, p: Port) -> Self {
        self.manifest.units.insert(p.name.clone(), p.unit.clone());
        self.manifest.output_schema.push(p);
        self
    }
    pub fn assumption(mut self, a: &str) -> Self {
        self.manifest.assumptions.push(a.to_string());
        self
    }
    /// Evidence level claimed for outputs (default `Computed`).
    pub fn evidence(mut self, e: EvidenceLevel) -> Self {
        self.evidence = e;
        self
    }
    /// Finish with a closure receiving the request and returning the output
    /// map.  Validity of the manifest structure is checked here.
    pub fn build<F>(self, f: F) -> Result<ClosureComponent>
    where
        F: Fn(&TaskRequest, &ExecutionControl) -> Result<IndexMap<String, serde_json::Value>>
            + Send
            + Sync
            + 'static,
    {
        self.manifest.validate_structure()?;
        let evidence = self.evidence;
        Ok(ClosureComponent {
            manifest: self.manifest,
            exec: Box::new(move |req, ctl| {
                let outputs = f(req, ctl)?;
                let mut res = TaskResult::new(req, evidence);
                res.outputs = outputs;
                Ok(res)
            }),
        })
    }
    /// Finish with a closure that builds the full `TaskResult` itself
    /// (useful to publish metrics and warnings).
    pub fn build_raw<F>(self, f: F) -> Result<ClosureComponent>
    where
        F: Fn(&TaskRequest, &ExecutionControl) -> Result<TaskResult> + Send + Sync + 'static,
    {
        self.manifest.validate_structure()?;
        Ok(ClosureComponent {
            manifest: self.manifest,
            exec: Box::new(f),
        })
    }
}

/// Shared handle to a component.
pub type SharedComponent = Arc<dyn Component>;

/// Factory producing a component instance.
pub type ComponentFactory = Arc<dyn Fn() -> Result<SharedComponent> + Send + Sync>;

/// Discovery table: name → factory, plus cached manifests.
#[derive(Clone, Default)]
pub struct ComponentSet {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Default)]
struct Inner {
    factories: IndexMap<String, ComponentFactory>,
    manifests: IndexMap<String, ComponentManifest>,
    instances: IndexMap<String, SharedComponent>,
}

impl ComponentSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a factory under the manifest name.  The factory is invoked
    /// once to obtain the manifest; the instance is cached and shared.
    pub fn register(
        &self,
        factory: impl Fn() -> Result<SharedComponent> + Send + Sync + 'static,
    ) -> Result<String> {
        let inst = factory()?;
        let manifest = inst.manifest().clone();
        manifest.validate_structure()?;
        let name = manifest.name.clone();
        let mut g = self.inner.write();
        if g.factories.contains_key(&name) {
            return Err(SimorphError::InvalidInput(format!(
                "component `{name}` already registered"
            )));
        }
        g.factories.insert(name.clone(), Arc::new(factory));
        g.manifests.insert(name.clone(), manifest);
        g.instances.insert(name.clone(), inst);
        Ok(name)
    }

    /// Register an already-built shared instance.
    pub fn register_instance(&self, c: SharedComponent) -> Result<String> {
        let shared = c.clone();
        self.register(move || Ok(shared.clone()))
    }

    pub fn get(&self, name: &str) -> Option<SharedComponent> {
        self.inner.read().instances.get(name).cloned()
    }

    pub fn get_by_id(&self, id: ComponentId) -> Option<SharedComponent> {
        let g = self.inner.read();
        g.manifests
            .iter()
            .find(|(_, m)| m.id == id)
            .and_then(|(n, _)| g.instances.get(n).cloned())
    }

    /// Fresh (non-shared) instance from the factory, e.g. for isolation.
    pub fn instantiate(&self, name: &str) -> Result<SharedComponent> {
        let f = self
            .inner
            .read()
            .factories
            .get(name)
            .cloned()
            .ok_or_else(|| SimorphError::NotFound(format!("component `{name}`")))?;
        f()
    }

    pub fn manifests(&self) -> Vec<ComponentManifest> {
        self.inner.read().manifests.values().cloned().collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.inner.read().manifests.keys().cloned().collect()
    }

    pub fn find_by_capability(&self, c: &Capability) -> Vec<ComponentManifest> {
        self.inner
            .read()
            .manifests
            .values()
            .filter(|m| m.has_capability(c))
            .cloned()
            .collect()
    }

    pub fn find_by_domain(&self, d: &str) -> Vec<ComponentManifest> {
        self.inner
            .read()
            .manifests
            .values()
            .filter(|m| m.supported_domains.iter().any(|x| x == d))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().manifests.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Serve a component over stdin/stdout using the SIMORPH JSON-lines
/// protocol.  Returns when stdin is closed or a `shutdown` message arrives.
/// Any Rust binary can call this in `main` to become an out-of-process
/// component.
pub fn serve_stdio(component: &dyn Component) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(component, stdin.lock(), stdout.lock())
}

/// Generic server over any reader/writer pair (used by tests).
pub fn serve<R: BufRead, W: Write>(
    component: &dyn Component,
    reader: R,
    mut writer: W,
) -> std::io::Result<()> {
    use protocol::{Request, Response};
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("malformed request: {e}"),
                    run_id: None,
                };
                writeln!(writer, "{}", serde_json::to_string(&resp).unwrap())?;
                writer.flush()?;
                continue;
            }
        };
        let resp = match req {
            Request::Manifest => Response::Manifest {
                manifest: Box::new(component.manifest().clone()),
            },
            Request::Execute { request } => {
                let ctl = ExecutionControl::default();
                match simorph_core::run_component(component, &request, &ctl) {
                    Ok(result) => Response::Result { result },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                        run_id: Some(request.run_id),
                    },
                }
            }
            Request::Cancel => Response::Ack,
            Request::Shutdown => {
                writeln!(writer, "{}", serde_json::to_string(&Response::Ack).unwrap())?;
                writer.flush()?;
                return Ok(());
            }
        };
        writeln!(writer, "{}", serde_json::to_string(&resp).unwrap())?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Request, Response};

    fn doubler() -> ClosureComponent {
        ComponentBuilder::new("test.doubler", "1.0.0", "simorph")
            .capability(Capability::AlgebraicSolver)
            .domain("test")
            .input(Port::scalar("x", "m"))
            .output(Port::scalar("y", "m"))
            .build(|req, _| {
                let x = req.get_f64("x")?;
                let mut out = IndexMap::new();
                out.insert("y".to_string(), serde_json::json!(2.0 * x));
                Ok(out)
            })
            .unwrap()
    }

    #[test]
    fn builder_and_execution() {
        let d = doubler();
        let req = TaskRequest::new(d.manifest().id).input("x", 2.5);
        let r = simorph_core::run_component(&d, &req, &ExecutionControl::default()).unwrap();
        assert_eq!(r.get_f64("y").unwrap(), 5.0);
        assert_eq!(d.manifest().units["x"], "m");
    }

    #[test]
    fn component_set_discovery() {
        let set = ComponentSet::new();
        set.register(|| Ok(Arc::new(doubler()) as SharedComponent))
            .unwrap();
        assert!(set
            .register(|| Ok(Arc::new(doubler()) as SharedComponent))
            .is_err());
        assert_eq!(set.names(), vec!["test.doubler".to_string()]);
        assert_eq!(
            set.find_by_capability(&Capability::AlgebraicSolver).len(),
            1
        );
        assert_eq!(set.find_by_domain("nope").len(), 0);
        let c = set.get("test.doubler").unwrap();
        assert!(set.get_by_id(c.manifest().id).is_some());
        assert!(set.instantiate("missing").is_err());
    }

    #[test]
    fn stdio_server_round_trip() {
        let d = doubler();
        let req = TaskRequest::new(d.manifest().id).input("x", 4.0);
        let input = format!(
            "{}\n{}\nnot json\n{}\n",
            serde_json::to_string(&Request::Manifest).unwrap(),
            serde_json::to_string(&Request::Execute {
                request: req.clone()
            })
            .unwrap(),
            serde_json::to_string(&Request::Shutdown).unwrap(),
        );
        let mut out = Vec::new();
        serve(&d, std::io::Cursor::new(input), &mut out).unwrap();
        let lines: Vec<Response> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 4);
        assert!(
            matches!(&lines[0], Response::Manifest { manifest } if manifest.name == "test.doubler")
        );
        assert!(
            matches!(&lines[1], Response::Result { result } if result.get_f64("y").unwrap() == 8.0)
        );
        assert!(matches!(&lines[2], Response::Error { .. }));
        assert!(matches!(&lines[3], Response::Ack));
    }
}
