//! The co-simulation slave contract.

use simorph_core::{ComponentManifest, Port, Result, SimorphError};
use simorph_simulation::Params;

/// Opaque saved state of a slave (numbers only, so that it is cheap to clone
/// and serialise for provenance).
pub type Snapshot = Vec<f64>;

/// FMI-like contract.  Vector ports are addressed element-wise as
/// `name[i]`; see [`expand_port`].
pub trait CosimSlave: Send + Sync {
    fn manifest(&self) -> &ComponentManifest;

    /// Reset to time `t0` with the given parameters.
    fn initialize(&mut self, t0: f64, params: &Params) -> Result<()>;

    /// Set an input port value (in the port's declared unit).
    fn set_input(&mut self, name: &str, value: f64) -> Result<()>;

    /// Advance from `t` to `t + dt` with the inputs currently set.
    fn do_step(&mut self, t: f64, dt: f64) -> Result<()>;

    /// Read an output port value (in the port's declared unit).
    fn get_output(&self, name: &str) -> Result<f64>;

    /// `true` when outputs depend algebraically on the inputs set for the
    /// current step (as opposed to depending on internal state only).
    /// Used to detect algebraic loops.
    fn has_direct_feedthrough(&self) -> bool {
        true
    }

    fn supports_rollback(&self) -> bool {
        false
    }

    fn save_state(&self) -> Result<Snapshot> {
        Err(SimorphError::NotImplemented(format!(
            "slave `{}` does not support state save",
            self.manifest().name
        )))
    }

    fn restore_state(&mut self, _snapshot: &Snapshot) -> Result<()> {
        Err(SimorphError::NotImplemented(format!(
            "slave `{}` does not support state restore",
            self.manifest().name
        )))
    }
}

/// Base name of a possibly indexed port name (`"pos[2]"` -> `"pos"`).
pub fn port_base(name: &str) -> &str {
    match name.find('[') {
        Some(i) => &name[..i],
        None => name,
    }
}

/// Scalar names of a port: `name` for scalars, `name[0]..name[n-1]` for
/// vectors (only one-dimensional shapes are expanded).
pub fn expand_port(p: &Port) -> Vec<String> {
    match p.shape.as_slice() {
        [] => vec![p.name.clone()],
        [n] => (0..*n).map(|i| format!("{}[{i}]", p.name)).collect(),
        _ => vec![p.name.clone()],
    }
}

/// Find the declared port (input or output) for a scalar name.
pub fn find_port<'a>(ports: &'a [Port], name: &str) -> Option<&'a Port> {
    let base = port_base(name);
    ports.iter().find(|p| p.name == base)
}
