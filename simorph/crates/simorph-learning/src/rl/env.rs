//! Environments with the dynamics of Gymnasium `CartPole-v1` and `Pendulum-v1`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionSpace {
    Discrete(usize),
    /// Box with per-dimension bounds.
    Continuous {
        low: Vec<f64>,
        high: Vec<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Discrete(usize),
    Continuous(Vec<f64>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub observation: Vec<f64>,
    pub reward: f64,
    pub terminated: bool,
    pub truncated: bool,
}

pub trait Environment: Send {
    fn name(&self) -> &str;
    fn observation_dim(&self) -> usize;
    fn action_space(&self) -> ActionSpace;
    fn reset(&mut self, seed: u64) -> Vec<f64>;
    fn step(&mut self, action: &Action) -> Step;
}

/// Classic cart-pole (Barto, Sutton & Anderson 1983), constants and Euler
/// integration identical to Gymnasium `CartPole-v1`; episodes truncate at 500 steps.
#[derive(Debug, Clone)]
pub struct CartPole {
    pub state: [f64; 4],
    pub steps: usize,
    pub max_steps: usize,
}

impl CartPole {
    pub const GRAVITY: f64 = 9.8;
    pub const MASS_CART: f64 = 1.0;
    pub const MASS_POLE: f64 = 0.1;
    pub const LENGTH: f64 = 0.5; // half pole length
    pub const FORCE_MAG: f64 = 10.0;
    pub const TAU: f64 = 0.02;
    pub const THETA_THRESHOLD: f64 = 12.0 * 2.0 * std::f64::consts::PI / 360.0;
    pub const X_THRESHOLD: f64 = 2.4;

    pub fn new() -> Self {
        Self {
            state: [0.0; 4],
            steps: 0,
            max_steps: 500,
        }
    }

    /// One Euler step of the dynamics (no termination logic).
    pub fn dynamics(state: [f64; 4], action: usize) -> [f64; 4] {
        let [x, x_dot, theta, theta_dot] = state;
        let force = if action == 1 {
            Self::FORCE_MAG
        } else {
            -Self::FORCE_MAG
        };
        let total_mass = Self::MASS_CART + Self::MASS_POLE;
        let polemass_length = Self::MASS_POLE * Self::LENGTH;
        let (s, c) = theta.sin_cos();
        let temp = (force + polemass_length * theta_dot * theta_dot * s) / total_mass;
        let thetaacc = (Self::GRAVITY * s - c * temp)
            / (Self::LENGTH * (4.0 / 3.0 - Self::MASS_POLE * c * c / total_mass));
        let xacc = temp - polemass_length * thetaacc * c / total_mass;
        [
            x + Self::TAU * x_dot,
            x_dot + Self::TAU * xacc,
            theta + Self::TAU * theta_dot,
            theta_dot + Self::TAU * thetaacc,
        ]
    }
}

impl Default for CartPole {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment for CartPole {
    fn name(&self) -> &str {
        "CartPole-v1"
    }
    fn observation_dim(&self) -> usize {
        4
    }
    fn action_space(&self) -> ActionSpace {
        ActionSpace::Discrete(2)
    }
    fn reset(&mut self, seed: u64) -> Vec<f64> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for v in &mut self.state {
            *v = rng.gen_range(-0.05..0.05);
        }
        self.steps = 0;
        self.state.to_vec()
    }
    fn step(&mut self, action: &Action) -> Step {
        let a = match action {
            Action::Discrete(a) => *a,
            Action::Continuous(v) => usize::from(v.first().copied().unwrap_or(0.0) > 0.0),
        };
        self.state = Self::dynamics(self.state, a);
        self.steps += 1;
        let [x, _, theta, _] = self.state;
        let terminated = x.abs() > Self::X_THRESHOLD || theta.abs() > Self::THETA_THRESHOLD;
        Step {
            observation: self.state.to_vec(),
            reward: 1.0,
            terminated,
            truncated: !terminated && self.steps >= self.max_steps,
        }
    }
}

/// Inverted pendulum swing-up with the dynamics of Gymnasium `Pendulum-v1`;
/// episodes truncate at 200 steps; observation `[cos θ, sin θ, θ̇]`.
#[derive(Debug, Clone)]
pub struct Pendulum {
    pub theta: f64,
    pub theta_dot: f64,
    pub steps: usize,
    pub max_steps: usize,
}

impl Pendulum {
    pub const MAX_SPEED: f64 = 8.0;
    pub const MAX_TORQUE: f64 = 2.0;
    pub const DT: f64 = 0.05;
    pub const G: f64 = 10.0;
    pub const M: f64 = 1.0;
    pub const L: f64 = 1.0;

    pub fn new() -> Self {
        Self {
            theta: 0.0,
            theta_dot: 0.0,
            steps: 0,
            max_steps: 200,
        }
    }
    fn angle_normalize(x: f64) -> f64 {
        use std::f64::consts::PI;
        (x + PI).rem_euclid(2.0 * PI) - PI
    }
    /// One step of the dynamics; returns (θ', θ̇', cost).
    pub fn dynamics(theta: f64, theta_dot: f64, u: f64) -> (f64, f64, f64) {
        let u = u.clamp(-Self::MAX_TORQUE, Self::MAX_TORQUE);
        let cost =
            Self::angle_normalize(theta).powi(2) + 0.1 * theta_dot * theta_dot + 0.001 * u * u;
        let mut new_dot = theta_dot
            + (3.0 * Self::G / (2.0 * Self::L) * theta.sin()
                + 3.0 / (Self::M * Self::L * Self::L) * u)
                * Self::DT;
        new_dot = new_dot.clamp(-Self::MAX_SPEED, Self::MAX_SPEED);
        (theta + new_dot * Self::DT, new_dot, cost)
    }
    fn obs(&self) -> Vec<f64> {
        vec![self.theta.cos(), self.theta.sin(), self.theta_dot]
    }
}

impl Default for Pendulum {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment for Pendulum {
    fn name(&self) -> &str {
        "Pendulum-v1"
    }
    fn observation_dim(&self) -> usize {
        3
    }
    fn action_space(&self) -> ActionSpace {
        ActionSpace::Continuous {
            low: vec![-Self::MAX_TORQUE],
            high: vec![Self::MAX_TORQUE],
        }
    }
    fn reset(&mut self, seed: u64) -> Vec<f64> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        self.theta = rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI);
        self.theta_dot = rng.gen_range(-1.0..1.0);
        self.steps = 0;
        self.obs()
    }
    fn step(&mut self, action: &Action) -> Step {
        let u = match action {
            Action::Continuous(v) => v.first().copied().unwrap_or(0.0),
            Action::Discrete(a) => *a as f64,
        };
        let (th, thd, cost) = Self::dynamics(self.theta, self.theta_dot, u);
        self.theta = th;
        self.theta_dot = thd;
        self.steps += 1;
        Step {
            observation: self.obs(),
            reward: -cost,
            terminated: false,
            truncated: self.steps >= self.max_steps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartpole_terminates_and_pendulum_truncates() {
        let mut e = CartPole::new();
        e.reset(0);
        let mut n = 0;
        loop {
            n += 1;
            let s = e.step(&Action::Discrete(1));
            if s.terminated || s.truncated {
                assert!(s.terminated);
                break;
            }
        }
        assert!(n < 50, "always pushing right must fail quickly ({n})");
        let mut p = Pendulum::new();
        p.reset(1);
        let mut k = 0;
        while !p.step(&Action::Continuous(vec![0.0])).truncated {
            k += 1;
        }
        assert_eq!(k + 1, 200);
    }

    /// Cross-check against Gymnasium when it is installed.
    #[test]
    fn dynamics_match_gymnasium() {
        if let Err(reason) = simorph_adapter_python_bridge::python_module_available("gymnasium") {
            eprintln!("SKIP gymnasium cross-check: {reason}");
            return;
        }
        let code = r#"
import json, numpy as np, gymnasium as gym
e = gym.make("CartPole-v1").unwrapped; e.reset(seed=0)
out = {}
e.state = np.array([0.1, -0.3, 0.05, 0.7], dtype=np.float64); e.step(1); out["cp1"] = [float(v) for v in e.state]
e.state = np.array([0.1, -0.3, 0.05, 0.7], dtype=np.float64); e.step(0); out["cp0"] = [float(v) for v in e.state]
p = gym.make("Pendulum-v1").unwrapped; p.reset(seed=0)
p.state = np.array([2.5, -1.2]); _, r, _, _, _ = p.step(np.array([0.7])); out["pd"] = [float(p.state[0]), float(p.state[1]), float(r)]
print(json.dumps(out))
"#;
        let out = std::process::Command::new(simorph_adapter_python_bridge::python_interpreter())
            .args(["-I", "-c", code])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let s0 = [0.1, -0.3, 0.05, 0.7];
        for (a, key) in [(1usize, "cp1"), (0, "cp0")] {
            let mine = CartPole::dynamics(s0, a);
            for (m, g) in mine.iter().zip(v[key].as_array().unwrap()) {
                assert!((m - g.as_f64().unwrap()).abs() < 1e-9, "{key}: {m} vs {g}");
            }
        }
        let (th, thd, cost) = Pendulum::dynamics(2.5, -1.2, 0.7);
        let g = v["pd"].as_array().unwrap();
        // gymnasium stores the pendulum state as float32-compatible arrays; allow 1e-6
        assert!(
            (th - g[0].as_f64().unwrap()).abs() < 1e-6
                && (thd - g[1].as_f64().unwrap()).abs() < 1e-6,
            "{th},{thd} vs {g:?}"
        );
        assert!((-cost - g[2].as_f64().unwrap()).abs() < 1e-6);
    }
}
