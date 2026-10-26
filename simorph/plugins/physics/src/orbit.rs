//! Two-body orbital mechanics in an Earth-centred inertial frame with an
//! optional J2 zonal perturbation and a cylindrical eclipse model.
//!
//! State: `x, y, z` (m), `vx, vy, vz` (m/s).
//!
//! Equations: `r̈ = -μ r / |r|³ + a_J2`, with
//! `a_J2 = (3/2) J2 μ R² / r⁵ · [x(5z²/r² − 1), y(5z²/r² − 1), z(5z²/r² − 3)]`.

use indexmap::IndexMap;
use simorph_core::{ReferenceCase, Result};
use simorph_cosimulation::OdeSlave;
use simorph_simulation::model::param;
use simorph_simulation::{DynamicalModel, OdeComponent, Params};
use std::sync::Arc;

pub const NAME: &str = "physics.orbit.two_body";
/// Earth gravitational parameter (m³/s²), WGS-84.
pub const MU_EARTH: f64 = 3.986_004_418e14;
/// Earth equatorial radius (m), WGS-84.
pub const R_EARTH: f64 = 6_378_137.0;
/// Earth J2 (EGM2008).
pub const J2_EARTH: f64 = 1.082_626_68e-3;

pub struct TwoBody;

impl TwoBody {
    /// Circular orbit period `2π√(a³/μ)`.
    pub fn circular_period(a: f64, mu: f64) -> f64 {
        2.0 * std::f64::consts::PI * (a * a * a / mu).sqrt()
    }
    /// Secular rate of the right ascension of the ascending node due to J2
    /// (rad/s): `-(3/2) n J2 (R/p)² cos i`, `p = a(1−e²)`.
    pub fn j2_nodal_rate(a: f64, e: f64, inc: f64, mu: f64, r_eq: f64, j2: f64) -> f64 {
        let n = (mu / (a * a * a)).sqrt();
        let p = a * (1.0 - e * e);
        -1.5 * n * j2 * (r_eq / p).powi(2) * inc.cos()
    }
    /// Specific mechanical energy including the J2 potential
    /// `v²/2 − μ/r + μ J2 R² (3z²/r² − 1) / (2r³)`; this is the conserved
    /// integral of the modelled dynamics.
    pub fn energy_with_j2(x: &[f64], mu: f64, r_eq: f64, j2: f64) -> f64 {
        let r2 = x[0] * x[0] + x[1] * x[1] + x[2] * x[2];
        let r = r2.sqrt();
        let v2 = x[3] * x[3] + x[4] * x[4] + x[5] * x[5];
        0.5 * v2 - mu / r + mu * j2 * r_eq * r_eq * (3.0 * x[2] * x[2] / r2 - 1.0) / (2.0 * r2 * r)
    }
    /// Specific orbital energy (two-body), angular momentum vector, RAAN, inclination.
    pub fn elements(x: &[f64], mu: f64) -> (f64, [f64; 3], f64, f64) {
        let r = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        let v2 = x[3] * x[3] + x[4] * x[4] + x[5] * x[5];
        let energy = 0.5 * v2 - mu / r;
        let h = [
            x[1] * x[5] - x[2] * x[4],
            x[2] * x[3] - x[0] * x[5],
            x[0] * x[4] - x[1] * x[3],
        ];
        let hm = (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt();
        let inc = (h[2] / hm).acos();
        let raan = h[0].atan2(-h[1]);
        (energy, h, raan, inc)
    }
}

impl DynamicalModel for TwoBody {
    fn name(&self) -> &str {
        NAME
    }
    fn state_dim(&self) -> usize {
        6
    }
    fn state_names(&self) -> Vec<(String, String)> {
        ["x", "y", "z"]
            .iter()
            .map(|s| (s.to_string(), "m".to_string()))
            .chain(
                ["vx", "vy", "vz"]
                    .iter()
                    .map(|s| (s.to_string(), "m/s".to_string())),
            )
            .collect()
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![
            ("mu".into(), "m^3/s^2".into()),
            ("r_eq".into(), "m".into()),
            ("j2".into(), "1".into()),
            ("sun_x".into(), "1".into()),
            ("sun_y".into(), "1".into()),
            ("sun_z".into(), "1".into()),
            ("x0".into(), "m".into()),
            ("y0".into(), "m".into()),
            ("z0".into(), "m".into()),
            ("vx0".into(), "m/s".into()),
            ("vy0".into(), "m/s".into()),
            ("vz0".into(), "m/s".into()),
        ]
    }
    fn output_names(&self) -> Vec<(String, String)> {
        vec![
            ("altitude".into(), "m".into()),
            ("eclipse".into(), "1".into()),
            ("speed".into(), "m/s".into()),
            ("specific_energy".into(), "J/kg".into()), // includes the J2 potential
            ("raan".into(), "rad".into()),
            ("inclination".into(), "rad".into()),
        ]
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        ["x0", "y0", "z0", "vx0", "vy0", "vz0"]
            .iter()
            .map(|k| p.get(*k).copied().unwrap_or(0.0))
            .collect()
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        let mu = param(p, "mu")?;
        let j2 = p.get("j2").copied().unwrap_or(0.0);
        let r_eq = p.get("r_eq").copied().unwrap_or(R_EARTH);
        let r2 = x[0] * x[0] + x[1] * x[1] + x[2] * x[2];
        let r = r2.sqrt();
        let r3 = r2 * r;
        dx[0] = x[3];
        dx[1] = x[4];
        dx[2] = x[5];
        let mut ax = -mu * x[0] / r3;
        let mut ay = -mu * x[1] / r3;
        let mut az = -mu * x[2] / r3;
        if j2 != 0.0 {
            let f = 1.5 * j2 * mu * r_eq * r_eq / (r2 * r3);
            let z2r2 = 5.0 * x[2] * x[2] / r2;
            ax += f * x[0] * (z2r2 - 1.0);
            ay += f * x[1] * (z2r2 - 1.0);
            az += f * x[2] * (z2r2 - 3.0);
        }
        dx[3] = ax;
        dx[4] = ay;
        dx[5] = az;
        Ok(())
    }
    fn outputs(&self, _t: f64, x: &[f64], p: &Params) -> IndexMap<String, f64> {
        let mu = p.get("mu").copied().unwrap_or(MU_EARTH);
        let r_eq = p.get("r_eq").copied().unwrap_or(R_EARTH);
        let r = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        let (sx, sy, sz) = (
            p.get("sun_x").copied().unwrap_or(1.0),
            p.get("sun_y").copied().unwrap_or(0.0),
            p.get("sun_z").copied().unwrap_or(0.0),
        );
        let sn = (sx * sx + sy * sy + sz * sz).sqrt().max(1e-300);
        let (sx, sy, sz) = (sx / sn, sy / sn, sz / sn);
        let dot = x[0] * sx + x[1] * sy + x[2] * sz;
        let perp =
            ((x[0] - dot * sx).powi(2) + (x[1] - dot * sy).powi(2) + (x[2] - dot * sz).powi(2))
                .sqrt();
        let eclipse = if dot < 0.0 && perp < r_eq { 1.0 } else { 0.0 };
        let (_, _, raan, inc) = Self::elements(x, mu);
        let j2 = p.get("j2").copied().unwrap_or(0.0);
        IndexMap::from([
            ("altitude".to_string(), r - r_eq),
            ("eclipse".to_string(), eclipse),
            (
                "speed".to_string(),
                (x[3] * x[3] + x[4] * x[4] + x[5] * x[5]).sqrt(),
            ),
            (
                "specific_energy".to_string(),
                Self::energy_with_j2(x, mu, r_eq, j2),
            ),
            ("raan".to_string(), raan),
            ("inclination".to_string(), inc),
        ])
    }
    fn assumptions(&self) -> Vec<String> {
        vec![
            "Point-mass central body; only the J2 zonal harmonic is modelled (no drag, SRP, third bodies or higher harmonics).".into(),
            "Inertial, non-rotating frame; the Sun direction is a fixed unit vector over the simulated span.".into(),
            "Eclipse uses a cylindrical shadow of the central body (no penumbra).".into(),
        ]
    }
    fn validity_ranges(&self) -> Vec<(String, (f64, f64))> {
        vec![("j2".into(), (0.0, 1e-2)), ("mu".into(), (1e10, 1e30))]
    }
    fn domains(&self) -> Vec<String> {
        vec!["aerospace".into(), "orbital-mechanics".into()]
    }
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        let a = 7.0e6;
        let v = (MU_EARTH / a).sqrt();
        let t = TwoBody::circular_period(a, MU_EARTH);
        vec![ReferenceCase {
            name: "circular orbit returns to its initial position after one period".into(),
            inputs: serde_json::json!({"mu": MU_EARTH, "r_eq": R_EARTH, "j2": 0.0, "sun_x": 1.0, "sun_y": 0.0, "sun_z": 0.0,
                "x0": a, "y0": 0.0, "z0": 0.0, "vx0": 0.0, "vy0": v, "vz0": 0.0, "t_end": t}),
            expected_outputs: serde_json::json!({"final_x": a, "final_y": 0.0, "final_z": 0.0, "altitude": a - R_EARTH}),
            rtol: 1e-6,
            atol: 10.0,
            source: "Kepler's third law, T = 2π√(a³/μ)".into(),
        }]
    }
}

pub fn component() -> Box<dyn simorph_core::Component> {
    Box::new(OdeComponent::with_defaults(
        Arc::new(TwoBody),
        crate::VERSION,
        crate::PROVIDER,
        crate::default_solver(),
    ))
}

/// Slave with no inputs (the orbit is autonomous); outputs `x,y,z,vx,vy,vz,
/// altitude, eclipse, speed, specific_energy, raan, inclination`.
pub fn slave() -> Result<OdeSlave> {
    OdeSlave::new(Arc::new(TwoBody), &[], crate::default_solver(), false)
}

/// Earth-orbit parameter set for a circular orbit of semi-major axis `a`
/// and inclination `inc` (rad), starting at the ascending node.
pub fn circular_params(a: f64, inc: f64, j2: f64) -> Params {
    let v = (MU_EARTH / a).sqrt();
    [
        ("mu", MU_EARTH),
        ("r_eq", R_EARTH),
        ("j2", j2),
        ("sun_x", 1.0),
        ("sun_y", 0.0),
        ("sun_z", 0.0),
        ("x0", a),
        ("y0", 0.0),
        ("z0", 0.0),
        ("vx0", 0.0),
        ("vy0", v * inc.cos()),
        ("vz0", v * inc.sin()),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect()
}
