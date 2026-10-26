//! Solar array electrical power.
//!
//! `P = irradiance · area · efficiency · (1 + temp_coeff (T − t_ref)) · max(0, cos_incidence) · (1 − eclipse)`.

use indexmap::IndexMap;
use simorph_core::{
    Capability, Component, ComponentManifest, EvidenceLevel, ExecutionControl, Port, ReferenceCase,
    Result, TaskRequest, TaskResult, UncertaintyModel, ValidationProtocol,
};
use simorph_cosimulation::AlgebraicSlave;
use simorph_simulation::model::param;
use simorph_simulation::Params;

pub const NAME: &str = "physics.solar.array";
/// Solar constant at 1 au (W/m²).
pub const SOLAR_CONSTANT: f64 = 1361.0;

/// Power (W).
pub fn power(
    irradiance: f64,
    cos_incidence: f64,
    temperature: f64,
    eclipse: f64,
    area: f64,
    efficiency: f64,
    temp_coeff: f64,
    t_ref: f64,
) -> f64 {
    let derate = (1.0 + temp_coeff * (temperature - t_ref)).max(0.0);
    irradiance
        * area
        * efficiency
        * derate
        * cos_incidence.max(0.0)
        * (1.0 - eclipse.clamp(0.0, 1.0))
}

fn power_from(p: &Params) -> Result<f64> {
    Ok(power(
        param(p, "irradiance")?,
        param(p, "cos_incidence")?,
        param(p, "temperature")?,
        param(p, "eclipse")?,
        param(p, "area")?,
        param(p, "efficiency")?,
        param(p, "temp_coeff")?,
        param(p, "t_ref")?,
    ))
}

fn input_ports() -> Vec<Port> {
    vec![
        Port::scalar("irradiance", "W/m^2").with_range(0.0, 1e5),
        Port::scalar("cos_incidence", "1").with_range(-1.0, 1.0),
        Port::scalar("temperature", "K").with_range(0.0, 1000.0),
        Port::scalar("eclipse", "1").with_range(0.0, 1.0),
    ]
}
fn param_ports() -> Vec<Port> {
    vec![
        Port::scalar("area", "m^2").with_range(0.0, 1e6),
        Port::scalar("efficiency", "1").with_range(0.0, 1.0),
        Port::scalar("temp_coeff", "1/K").with_range(-0.1, 0.1),
        Port::scalar("t_ref", "K").with_range(0.0, 1000.0),
    ]
}

fn manifest(include_params_as_inputs: bool) -> ComponentManifest {
    let mut inputs = input_ports();
    if include_params_as_inputs {
        inputs.extend(param_ports());
    }
    let mut m = ComponentManifest::in_process(NAME, crate::VERSION, crate::PROVIDER)
        .with_capabilities(vec![Capability::AlgebraicSolver, Capability::CosimulationSlave])
        .with_domains(&["aerospace", "energy", "photovoltaics"])
        .with_inputs(inputs)
        .with_outputs(vec![Port::scalar("power", "W")])
        .with_assumptions(&[
            "Linear temperature derating around t_ref; no spectral or low-irradiance effects.",
            "Cosine law for incidence (flat panel, no shading, no cover-glass losses).",
            "Maximum-power-point tracking is ideal.",
        ])
        .with_validation(ValidationProtocol {
            reference_cases: vec![ReferenceCase {
                name: "1 m², 30 % at 1361 W/m², normal incidence, reference temperature".into(),
                inputs: serde_json::json!({"irradiance": SOLAR_CONSTANT, "cos_incidence": 1.0, "temperature": 301.15, "eclipse": 0.0,
                    "area": 1.0, "efficiency": 0.3, "temp_coeff": -0.004, "t_ref": 301.15}),
                expected_outputs: serde_json::json!({"power": 0.3 * SOLAR_CONSTANT}),
                rtol: 1e-12,
                atol: 0.0,
                source: "definition of conversion efficiency".into(),
            }],
            invariants: vec!["power >= 0".into()],
            convergence_order: None,
        });
    m.uncertainty_model = UncertaintyModel::None;
    m
}

pub struct SolarArray {
    manifest: ComponentManifest,
}

impl Component for SolarArray {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, _ctl: &ExecutionControl) -> Result<TaskResult> {
        let p: Params = self
            .manifest
            .input_schema
            .iter()
            .map(|port| req.get_f64(&port.name).map(|v| (port.name.clone(), v)))
            .collect::<Result<_>>()?;
        Ok(TaskResult::new(req, EvidenceLevel::Computed).output("power", power_from(&p)?))
    }
}

pub fn component() -> Box<dyn Component> {
    Box::new(SolarArray {
        manifest: manifest(true),
    })
}

/// Slave with inputs `irradiance`, `cos_incidence`, `temperature`,
/// `eclipse` and parameters `area`, `efficiency`, `temp_coeff`, `t_ref`.
pub fn slave() -> AlgebraicSlave {
    AlgebraicSlave::new(manifest(false), |p| {
        Ok(IndexMap::from([("power".to_string(), power_from(p)?)]))
    })
}
