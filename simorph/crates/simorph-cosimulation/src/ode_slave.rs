//! Wrap a continuous-time model as a co-simulation slave.

use crate::slave::{CosimSlave, Snapshot};
use simorph_core::{
    Capability, ComponentManifest, ExecutionControl, Port, Result, SimorphError, UncertaintyModel,
};
use simorph_simulation::{simulate, DynamicalModel, Params, SolverOptions};
use std::sync::Arc;

pub struct OdeSlave {
    model: Arc<dyn DynamicalModel>,
    manifest: ComponentManifest,
    opts: SolverOptions,
    params: Params,
    state: Vec<f64>,
    t: f64,
    feedthrough: bool,
}

impl OdeSlave {
    /// `input_names` are the model parameters exposed as input ports; the
    /// remaining parameters are fixed at `initialize`.  `feedthrough` must be
    /// `true` when the model's `outputs()` depend on those inputs.
    pub fn new(
        model: Arc<dyn DynamicalModel>,
        input_names: &[&str],
        opts: SolverOptions,
        feedthrough: bool,
    ) -> Result<Self> {
        let pnames = model.parameter_names();
        let mut inputs = vec![];
        for n in input_names {
            let (name, unit) = pnames.iter().find(|(p, _)| p == n).ok_or_else(|| {
                SimorphError::InvalidInput(format!(
                    "`{n}` is not a parameter of `{}`",
                    model.name()
                ))
            })?;
            inputs.push(Port::scalar(name.clone(), unit.clone()));
        }
        let mut outputs: Vec<Port> = model
            .state_names()
            .into_iter()
            .map(|(n, u)| Port::scalar(n, u))
            .collect();
        for (n, u) in model.output_names() {
            outputs.push(Port::scalar(n, u));
        }
        let mut manifest = ComponentManifest::in_process(model.name(), "0.1.0", "simorph")
            .with_capabilities(vec![
                Capability::OdeIntegration,
                Capability::CosimulationSlave,
            ])
            .with_inputs(inputs)
            .with_outputs(outputs);
        manifest.supported_domains = model.domains();
        manifest.assumptions = model.assumptions();
        for (p, r) in model.validity_ranges() {
            manifest.validity_domain.ranges.insert(p, r);
        }
        manifest.uncertainty_model = UncertaintyModel::RelativeErrorBound { rtol: opts.rtol };
        Ok(Self {
            model,
            manifest,
            opts,
            params: Params::new(),
            state: vec![],
            t: 0.0,
            feedthrough,
        })
    }

    pub fn state(&self) -> &[f64] {
        &self.state
    }
    pub fn params(&self) -> &Params {
        &self.params
    }
}

impl CosimSlave for OdeSlave {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn initialize(&mut self, t0: f64, params: &Params) -> Result<()> {
        self.params = params.clone();
        for p in &self.manifest.input_schema {
            self.params.entry(p.name.clone()).or_insert(0.0);
        }
        for (name, _) in self.model.parameter_names() {
            if !self.params.contains_key(&name) {
                return Err(SimorphError::InvalidInput(format!(
                    "slave `{}`: parameter `{name}` not provided",
                    self.manifest.name
                )));
            }
        }
        self.t = t0;
        self.state = self.model.initial_state(&self.params);
        Ok(())
    }
    fn set_input(&mut self, name: &str, value: f64) -> Result<()> {
        if !self.manifest.input_schema.iter().any(|p| p.name == name) {
            return Err(SimorphError::NotFound(format!(
                "input `{name}` of `{}`",
                self.manifest.name
            )));
        }
        self.params.insert(name.to_string(), value);
        Ok(())
    }
    fn do_step(&mut self, t: f64, dt: f64) -> Result<()> {
        if dt <= 0.0 {
            // Zero-length step: outputs are re-evaluated from the current
            // state and inputs without integrating.
            self.t = t;
            return Ok(());
        }
        let mut o = self.opts.clone();
        if o.solver.is_adaptive() {
            o.dt = o.dt.min(dt);
        } else {
            // Fixed-step methods take an integer number of sub-steps.
            let n = (dt / o.dt).ceil().max(1.0);
            o.dt = dt / n;
        }
        o.record_every = usize::MAX;
        let tr = simulate(
            self.model.as_ref(),
            &self.params,
            Some(&self.state),
            (t, t + dt),
            &o,
            &ExecutionControl::default(),
        )?;
        self.state = tr.final_state().to_vec();
        self.t = t + dt;
        Ok(())
    }
    fn get_output(&self, name: &str) -> Result<f64> {
        if let Some(i) = self.model.state_names().iter().position(|(n, _)| n == name) {
            return Ok(self.state[i]);
        }
        self.model
            .outputs(self.t, &self.state, &self.params)
            .get(name)
            .copied()
            .ok_or_else(|| {
                SimorphError::NotFound(format!("output `{name}` of `{}`", self.manifest.name))
            })
    }
    fn has_direct_feedthrough(&self) -> bool {
        self.feedthrough
    }
    fn supports_rollback(&self) -> bool {
        true
    }
    fn save_state(&self) -> Result<Snapshot> {
        let mut s = vec![self.t];
        s.extend_from_slice(&self.state);
        s.extend(self.params.values().copied());
        Ok(s)
    }
    fn restore_state(&mut self, s: &Snapshot) -> Result<()> {
        let n = self.state.len();
        if s.len() != 1 + n + self.params.len() {
            return Err(SimorphError::InvalidInput("snapshot size mismatch".into()));
        }
        self.t = s[0];
        self.state.copy_from_slice(&s[1..1 + n]);
        for (v, sv) in self.params.values_mut().zip(&s[1 + n..]) {
            *v = *sv;
        }
        Ok(())
    }
}
