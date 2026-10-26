//! Minimal tabular Q-learning over a discrete [`Environment`].
//!
//! This is the SIMORPH in-process baseline for reinforcement-learning
//! optimisation; large-scale RL (Ray RLlib, VERL) is integrated through
//! adapters, not reimplemented here.

use crate::problem::Method;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result};
use simorph_uncertainty::seeded;

/// Episodic environment with finite state and action spaces indexed by `usize`.
pub trait Environment {
    fn n_states(&self) -> usize;
    fn n_actions(&self) -> usize;
    fn reset(&mut self) -> usize;
    /// Returns (next_state, reward, done).
    fn step(&mut self, action: usize) -> (usize, f64, bool);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QLearningResult {
    pub method: Method,
    pub q: Vec<Vec<f64>>,
    pub greedy_policy: Vec<usize>,
    pub episodes: usize,
    /// Mean return over the last 100 training episodes.
    pub recent_return: f64,
}

#[derive(Debug, Clone)]
pub struct QOptions {
    pub alpha: f64,
    pub gamma: f64,
    pub epsilon: f64,
    pub epsilon_decay: f64,
    pub max_steps: usize,
}

impl Default for QOptions {
    fn default() -> Self {
        Self {
            alpha: 0.1,
            gamma: 0.95,
            epsilon: 1.0,
            epsilon_decay: 0.995,
            max_steps: 200,
        }
    }
}

pub fn q_learning<E: Environment>(
    env: &mut E,
    episodes: usize,
    opts: &QOptions,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<QLearningResult> {
    let mut rng = seeded(seed);
    let (ns, na) = (env.n_states(), env.n_actions());
    let mut q = vec![vec![0.0; na]; ns];
    let mut eps = opts.epsilon;
    let mut returns = vec![];
    for _ in 0..episodes {
        ctl.check()?;
        let mut s = env.reset();
        let mut total = 0.0;
        for _ in 0..opts.max_steps {
            let a = if rng.gen::<f64>() < eps {
                rng.gen_range(0..na)
            } else {
                argmax(&q[s])
            };
            let (s2, r, done) = env.step(a);
            total += r;
            let target = if done {
                r
            } else {
                r + opts.gamma * q[s2].iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            };
            q[s][a] += opts.alpha * (target - q[s][a]);
            s = s2;
            if done {
                break;
            }
        }
        returns.push(total);
        eps = (eps * opts.epsilon_decay).max(0.01);
    }
    let last = &returns[returns.len().saturating_sub(100)..];
    Ok(QLearningResult {
        method: Method::QLearning,
        greedy_policy: q.iter().map(|row| argmax(row)).collect(),
        q,
        episodes,
        recent_return: last.iter().sum::<f64>() / last.len().max(1) as f64,
    })
}

fn argmax(v: &[f64]) -> usize {
    let mut best = 0;
    for i in 1..v.len() {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1-D corridor of length 6: start at 0, goal at 5, actions left/right,
    /// reward -1 per step, +10 at goal.
    struct Corridor {
        pos: usize,
    }
    impl Environment for Corridor {
        fn n_states(&self) -> usize {
            6
        }
        fn n_actions(&self) -> usize {
            2
        }
        fn reset(&mut self) -> usize {
            self.pos = 0;
            0
        }
        fn step(&mut self, a: usize) -> (usize, f64, bool) {
            if a == 1 && self.pos < 5 {
                self.pos += 1;
            } else if a == 0 && self.pos > 0 {
                self.pos -= 1;
            }
            if self.pos == 5 {
                (5, 10.0, true)
            } else {
                (self.pos, -1.0, false)
            }
        }
    }

    #[test]
    fn learns_to_go_right() {
        let mut env = Corridor { pos: 0 };
        let r = q_learning(
            &mut env,
            500,
            &QOptions::default(),
            1,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(
            r.greedy_policy[..5].iter().all(|&a| a == 1),
            "{:?}",
            r.greedy_policy
        );
        // optimal return: 4 steps of -1 then +10 = 6
        assert!(r.recent_return > 5.0, "{}", r.recent_return);
    }
}
