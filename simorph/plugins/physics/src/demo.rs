//! Ready-made nanosatellite coupling: orbit → eclipse → solar power →
//! battery, with the solar array heating one node of a two-node thermal
//! network and an attitude slave running alongside.
//!
//! Signal names recorded by the master: `orbit.eclipse`, `orbit.altitude`,
//! `solar.power`, `battery.soc`, `battery.voltage`, `thermal.temperature_0`,
//! `thermal.temperature_1`, `attitude.pointing_error`, …

use crate::{attitude, battery, orbit, solar, thermal};
use simorph_core::Result;
use simorph_cosimulation::Coupling;
use simorph_simulation::Params;

/// Parameters of the demo that an experiment may vary.
#[derive(Debug, Clone)]
pub struct NanosatParams {
    /// Circular orbit altitude (m).
    pub altitude: f64,
    /// Inclination (rad).
    pub inclination: f64,
    /// Array area (m²).
    pub array_area: f64,
    pub array_efficiency: f64,
    /// Constant platform load (W).
    pub load_power: f64,
    pub battery_capacity_ah: f64,
    pub soc0: f64,
}

impl Default for NanosatParams {
    fn default() -> Self {
        Self {
            altitude: 500e3,
            inclination: 0.0,
            array_area: 0.06,
            array_efficiency: 0.28,
            load_power: 14.0,
            battery_capacity_ah: 5.0,
            soc0: 0.5,
        }
    }
}

pub fn nanosat_coupling(p: &NanosatParams) -> Result<Coupling> {
    let orbit_params =
        orbit::circular_params(orbit::R_EARTH + p.altitude, p.inclination, orbit::J2_EARTH);
    let solar_params: Params = [
        ("area", p.array_area),
        ("efficiency", p.array_efficiency),
        ("temp_coeff", -0.004),
        ("t_ref", 301.15),
        ("irradiance", solar::SOLAR_CONSTANT),
        ("cos_incidence", 1.0),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect();
    let battery_params: Params = [
        ("capacity_ah", p.battery_capacity_ah),
        ("r_internal", 0.05),
        ("v_ocv_min", 3.0),
        ("v_ocv_max", 4.2),
        ("soc0", p.soc0),
        ("power_out", p.load_power),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect();
    // Node 0: array (heated by absorbed, non-converted sunlight); node 1: body.
    let thermal_params: Params = [
        ("T_env", 2.7),
        ("T_space", 2.7),
        ("C_0", 400.0),
        ("G_env_0", 0.0),
        ("T0_0", 290.0),
        ("epsA_0", 0.06 * 0.85),
        ("C_1", 3000.0),
        ("G_env_1", 0.0),
        ("T0_1", 293.0),
        ("epsA_1", 0.2 * 0.6),
        ("G_0_1", 0.5),
        ("heat_in_1", p.load_power),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect();
    let net = thermal::ThermalNetwork::chain(2, true);
    Coupling::builder()
        .add_slave("orbit", Box::new(orbit::slave()?), orbit_params)?
        .add_slave("solar", Box::new(solar::slave()), solar_params)?
        .add_slave("battery", Box::new(battery::slave()?), battery_params)?
        .add_slave(
            "thermal",
            Box::new(thermal::slave_for(net)?),
            thermal_params,
        )?
        .add_slave(
            "attitude",
            Box::new(attitude::slave()?),
            attitude::default_params(),
        )?
        .connect("orbit.eclipse", "solar.eclipse")?
        .connect("thermal.temperature_0", "solar.temperature")?
        .connect("solar.power", "battery.power_in")?
        .connect("solar.power", "thermal.heat_in_0")?
        .build()
}
