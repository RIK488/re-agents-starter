//! Rigid-body attitude dynamics.
//!
//! State: unit quaternion `q0..q3` (scalar first, body → inertial) and body
//! angular rate `wx, wy, wz` (rad/s).
//!
//! Euler's equation `I ω̇ = τ − ω × (I ω)` with a principal-axes inertia
//! tensor, quaternion kinematics `q̇ = ½ q ⊗ (0, ω)`, and an optional PD
//! controller `τ_c = −kp · vec(q_e) − kd · ω` with `q_e = q_target⁻¹ ⊗ q`
//! (sign-corrected to the short rotation).

use indexmap::IndexMap;
use simorph_core::{ReferenceCase, Result};
use simorph_cosimulation::OdeSlave;
use simorph_simulation::model::{param, param_or};
use simorph_simulation::{DynamicalModel, OdeComponent, Params};
use std::sync::Arc;

pub const NAME: &str = "physics.attitude.rigid_body";

/// Hamilton product (scalar-first).
pub fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}
pub fn qconj(q: [f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}
/// Rotate a body vector into the inertial frame.
pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    [
        (1.0 - 2.0 * (y * y + z * z)) * v[0]
            + 2.0 * (x * y - w * z) * v[1]
            + 2.0 * (x * z + w * y) * v[2],
        2.0 * (x * y + w * z) * v[0]
            + (1.0 - 2.0 * (x * x + z * z)) * v[1]
            + 2.0 * (y * z - w * x) * v[2],
        2.0 * (x * z - w * y) * v[0]
            + 2.0 * (y * z + w * x) * v[1]
            + (1.0 - 2.0 * (x * x + y * y)) * v[2],
    ]
}

pub struct RigidBody;

impl RigidBody {
    pub fn inertial_angular_momentum(x: &[f64], p: &Params) -> [f64; 3] {
        let q = [x[0], x[1], x[2], x[3]];
        let h_body = [
            param_or(p, "Ixx", 1.0) * x[4],
            param_or(p, "Iyy", 1.0) * x[5],
            param_or(p, "Izz", 1.0) * x[6],
        ];
        rotate(q, h_body)
    }
    fn error_quaternion(x: &[f64], p: &Params) -> [f64; 4] {
        let q = [x[0], x[1], x[2], x[3]];
        let qt = [
            param_or(p, "q_target_0", 1.0),
            param_or(p, "q_target_1", 0.0),
            param_or(p, "q_target_2", 0.0),
            param_or(p, "q_target_3", 0.0),
        ];
        let n = (qt[0] * qt[0] + qt[1] * qt[1] + qt[2] * qt[2] + qt[3] * qt[3])
            .sqrt()
            .max(1e-300);
        let qt = [qt[0] / n, qt[1] / n, qt[2] / n, qt[3] / n];
        let mut qe = qmul(qconj(qt), q);
        if qe[0] < 0.0 {
            for v in &mut qe {
                *v = -*v;
            }
        }
        qe
    }
    /// Pointing error angle (rad) between current and target attitude.
    pub fn pointing_error(x: &[f64], p: &Params) -> f64 {
        let qe = Self::error_quaternion(x, p);
        let n = (qe[0] * qe[0] + qe[1] * qe[1] + qe[2] * qe[2] + qe[3] * qe[3]).sqrt();
        2.0 * (qe[0] / n).clamp(-1.0, 1.0).acos()
    }
}

impl DynamicalModel for RigidBody {
    fn name(&self) -> &str {
        NAME
    }
    fn state_dim(&self) -> usize {
        7
    }
    fn state_names(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> =
            (0..4).map(|i| (format!("q{i}"), "1".to_string())).collect();
        v.extend(
            ["wx", "wy", "wz"]
                .iter()
                .map(|s| (s.to_string(), "rad/s".to_string())),
        );
        v
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        let mut v = vec![
            ("Ixx".to_string(), "kg*m^2".to_string()),
            ("Iyy".into(), "kg*m^2".into()),
            ("Izz".into(), "kg*m^2".into()),
            ("kp".into(), "N*m".into()),
            ("kd".into(), "N*m*s".into()),
            ("ctrl_enabled".into(), "1".into()),
            ("torque_x".into(), "N*m".into()),
            ("torque_y".into(), "N*m".into()),
            ("torque_z".into(), "N*m".into()),
        ];
        for i in 0..4 {
            v.push((format!("q_target_{i}"), "1".into()));
            v.push((format!("q_init_{i}"), "1".into()));
        }
        for a in ["x", "y", "z"] {
            v.push((format!("w_init_{a}"), "rad/s".into()));
        }
        v
    }
    fn output_names(&self) -> Vec<(String, String)> {
        vec![
            ("pointing_error".into(), "rad".into()),
            ("q_norm".into(), "1".into()),
            ("h_inertial_x".into(), "kg*m^2/s".into()),
            ("h_inertial_y".into(), "kg*m^2/s".into()),
            ("h_inertial_z".into(), "kg*m^2/s".into()),
        ]
    }
    fn initial_state(&self, p: &Params) -> Vec<f64> {
        let mut q = [
            param_or(p, "q_init_0", 1.0),
            param_or(p, "q_init_1", 0.0),
            param_or(p, "q_init_2", 0.0),
            param_or(p, "q_init_3", 0.0),
        ];
        let n = (q.iter().map(|v| v * v).sum::<f64>()).sqrt().max(1e-300);
        for v in &mut q {
            *v /= n;
        }
        vec![
            q[0],
            q[1],
            q[2],
            q[3],
            param_or(p, "w_init_x", 0.0),
            param_or(p, "w_init_y", 0.0),
            param_or(p, "w_init_z", 0.0),
        ]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], p: &Params) -> Result<()> {
        let (ixx, iyy, izz) = (param(p, "Ixx")?, param(p, "Iyy")?, param(p, "Izz")?);
        let (w, qx, qy, qz) = (x[0], x[1], x[2], x[3]);
        let (wx, wy, wz) = (x[4], x[5], x[6]);
        dx[0] = -0.5 * (qx * wx + qy * wy + qz * wz);
        dx[1] = 0.5 * (w * wx + qy * wz - qz * wy);
        dx[2] = 0.5 * (w * wy + qz * wx - qx * wz);
        dx[3] = 0.5 * (w * wz + qx * wy - qy * wx);
        let mut tau = [
            param_or(p, "torque_x", 0.0),
            param_or(p, "torque_y", 0.0),
            param_or(p, "torque_z", 0.0),
        ];
        if param_or(p, "ctrl_enabled", 0.0) > 0.5 {
            let qe = Self::error_quaternion(x, p);
            let (kp, kd) = (param(p, "kp")?, param(p, "kd")?);
            tau[0] += -kp * qe[1] - kd * wx;
            tau[1] += -kp * qe[2] - kd * wy;
            tau[2] += -kp * qe[3] - kd * wz;
        }
        dx[4] = (tau[0] - (izz - iyy) * wy * wz) / ixx;
        dx[5] = (tau[1] - (ixx - izz) * wz * wx) / iyy;
        dx[6] = (tau[2] - (iyy - ixx) * wx * wy) / izz;
        Ok(())
    }
    fn outputs(&self, _t: f64, x: &[f64], p: &Params) -> IndexMap<String, f64> {
        let h = Self::inertial_angular_momentum(x, p);
        IndexMap::from([
            ("pointing_error".to_string(), Self::pointing_error(x, p)),
            (
                "q_norm".to_string(),
                (x[0] * x[0] + x[1] * x[1] + x[2] * x[2] + x[3] * x[3]).sqrt(),
            ),
            ("h_inertial_x".to_string(), h[0]),
            ("h_inertial_y".to_string(), h[1]),
            ("h_inertial_z".to_string(), h[2]),
        ])
    }
    fn assumptions(&self) -> Vec<String> {
        vec![
            "Rigid body with a diagonal (principal axes) inertia tensor; no flexible modes, fuel slosh or reaction-wheel dynamics.".into(),
            "The controller applies an ideal, unsaturated torque; actuator limits must be modelled externally.".into(),
            "Quaternion normalisation is not enforced during integration; the `q_norm` output reports the drift.".into(),
        ]
    }
    fn validity_ranges(&self) -> Vec<(String, (f64, f64))> {
        vec![
            ("Ixx".into(), (1e-9, 1e9)),
            ("Iyy".into(), (1e-9, 1e9)),
            ("Izz".into(), (1e-9, 1e9)),
            ("kp".into(), (0.0, 1e9)),
            ("kd".into(), (0.0, 1e9)),
        ]
    }
    fn domains(&self) -> Vec<String> {
        vec!["aerospace".into(), "robotics".into(), "control".into()]
    }
    fn reference_cases(&self) -> Vec<ReferenceCase> {
        // Pure spin about z at 0.1 rad/s for 10 s: q = (cos(0.5), 0, 0, sin(0.5)).
        vec![ReferenceCase {
            name: "pure spin about the z axis".into(),
            inputs: serde_json::json!({"Ixx": 1.0, "Iyy": 1.0, "Izz": 1.0, "kp": 0.0, "kd": 0.0, "ctrl_enabled": 0.0,
                "torque_x": 0.0, "torque_y": 0.0, "torque_z": 0.0,
                "q_target_0": 1.0, "q_target_1": 0.0, "q_target_2": 0.0, "q_target_3": 0.0,
                "q_init_0": 1.0, "q_init_1": 0.0, "q_init_2": 0.0, "q_init_3": 0.0,
                "w_init_x": 0.0, "w_init_y": 0.0, "w_init_z": 0.1, "t_end": 10.0}),
            expected_outputs: serde_json::json!({"final_q0": 0.5f64.cos(), "final_q3": 0.5f64.sin(), "final_wz": 0.1}),
            rtol: 1e-7,
            atol: 1e-9,
            source: "closed-form quaternion for constant-axis rotation".into(),
        }]
    }
}

pub fn component() -> Box<dyn simorph_core::Component> {
    Box::new(OdeComponent::with_defaults(
        Arc::new(RigidBody),
        crate::VERSION,
        crate::PROVIDER,
        crate::default_solver(),
    ))
}

/// Slave with torque inputs `torque_x/y/z` (N·m) and outputs `q0..q3`,
/// `wx,wy,wz`, `pointing_error`, `q_norm`, `h_inertial_*`.
pub fn slave() -> Result<OdeSlave> {
    OdeSlave::new(
        Arc::new(RigidBody),
        &["torque_x", "torque_y", "torque_z"],
        crate::default_solver(),
        false,
    )
}

/// Parameter set for a small satellite with the controller disabled.
pub fn default_params() -> Params {
    [
        ("Ixx", 0.05),
        ("Iyy", 0.06),
        ("Izz", 0.04),
        ("kp", 0.01),
        ("kd", 0.05),
        ("ctrl_enabled", 0.0),
        ("torque_x", 0.0),
        ("torque_y", 0.0),
        ("torque_z", 0.0),
        ("q_target_0", 1.0),
        ("q_target_1", 0.0),
        ("q_target_2", 0.0),
        ("q_target_3", 0.0),
        ("q_init_0", 1.0),
        ("q_init_1", 0.0),
        ("q_init_2", 0.0),
        ("q_init_3", 0.0),
        ("w_init_x", 0.0),
        ("w_init_y", 0.0),
        ("w_init_z", 0.0),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect()
}
