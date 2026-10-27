//! Stateless slave defined by a closure `inputs -> outputs`.

use crate::slave::{CosimSlave, Snapshot};
use indexmap::IndexMap;
use simorph_core::{ComponentManifest, Result, SimorphError};
use simorph_simulation::Params;

type Func = Box<dyn Fn(&Params) -> Result<IndexMap<String, f64>> + Send + Sync>;

pub struct AlgebraicSlave {
    manifest: ComponentManifest,
    func: Func,
    values: Params,
    outputs: IndexMap<String, f64>,
}

impl AlgebraicSlave {
    /// `manifest` declares the input/output ports; `func` maps the current
    /// parameters+inputs to the outputs.
    pub fn new(
        manifest: ComponentManifest,
        func: impl Fn(&Params) -> Result<IndexMap<String, f64>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            manifest,
            func: Box::new(func),
            values: Params::new(),
            outputs: IndexMap::new(),
        }
    }

    fn recompute(&mut self) -> Result<()> {
        self.outputs = (self.func)(&self.values)?;
        Ok(())
    }
}

impl CosimSlave for AlgebraicSlave {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn initialize(&mut self, _t0: f64, params: &Params) -> Result<()> {
        self.values = params.clone();
        for p in &self.manifest.input_schema {
            self.values.entry(p.name.clone()).or_insert(0.0);
        }
        self.recompute()
    }
    fn set_input(&mut self, name: &str, value: f64) -> Result<()> {
        if crate::slave::find_port(&self.manifest.input_schema, name).is_none() {
            return Err(SimorphError::NotFound(format!(
                "input `{name}` of `{}`",
                self.manifest.name
            )));
        }
        self.values.insert(name.to_string(), value);
        Ok(())
    }
    fn do_step(&mut self, _t: f64, _dt: f64) -> Result<()> {
        self.recompute()
    }
    fn get_output(&self, name: &str) -> Result<f64> {
        self.outputs.get(name).copied().ok_or_else(|| {
            SimorphError::NotFound(format!("output `{name}` of `{}`", self.manifest.name))
        })
    }
    fn supports_rollback(&self) -> bool {
        true
    }
    fn save_state(&self) -> Result<Snapshot> {
        Ok(self.values.values().copied().collect())
    }
    fn restore_state(&mut self, s: &Snapshot) -> Result<()> {
        for (v, s) in self.values.values_mut().zip(s) {
            *v = *s;
        }
        self.recompute()
    }
}
