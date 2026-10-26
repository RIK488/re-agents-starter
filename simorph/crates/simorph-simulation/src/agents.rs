//! Minimal multi-agent step scheduler.
//!
//! Agents are stepped at fixed intervals in a deterministic order (either
//! fixed or seeded-shuffled).  Each agent receives the shared environment
//! and a seeded random generator so that runs are reproducible.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use simorph_core::{ExecutionControl, Result};

/// An agent acting on a shared environment `E`.
pub trait Agent<E>: Send {
    fn step(&mut self, t: f64, env: &mut E, rng: &mut ChaCha8Rng);
}

/// Activation order of agents within a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Agents are stepped in registration order.
    Sequential,
    /// Agents are stepped in a seeded random order (re-drawn every step).
    RandomOrder,
}

pub struct AgentScheduler<E> {
    pub env: E,
    agents: Vec<Box<dyn Agent<E>>>,
    rng: ChaCha8Rng,
    time: f64,
    dt: f64,
    activation: Activation,
    steps: u64,
}

impl<E> AgentScheduler<E> {
    pub fn new(env: E, dt: f64, seed: u64, activation: Activation) -> Self {
        Self {
            env,
            agents: Vec::new(),
            rng: ChaCha8Rng::seed_from_u64(seed),
            time: 0.0,
            dt,
            activation,
            steps: 0,
        }
    }
    pub fn add(&mut self, agent: impl Agent<E> + 'static) {
        self.agents.push(Box::new(agent));
    }
    pub fn time(&self) -> f64 {
        self.time
    }
    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
    /// Advance one step.
    pub fn step(&mut self) {
        let mut order: Vec<usize> = (0..self.agents.len()).collect();
        if self.activation == Activation::RandomOrder {
            order.shuffle(&mut self.rng);
        }
        for i in order {
            self.agents[i].step(self.time, &mut self.env, &mut self.rng);
        }
        self.time += self.dt;
        self.steps += 1;
    }
    /// Run `n` steps honouring cancellation.
    pub fn run(&mut self, n: u64, ctl: &ExecutionControl) -> Result<()> {
        for _ in 0..n {
            ctl.check()?;
            self.step();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    struct Walker {
        pos: f64,
    }
    impl Agent<Vec<f64>> for Walker {
        fn step(&mut self, _t: f64, env: &mut Vec<f64>, rng: &mut ChaCha8Rng) {
            self.pos += if rng.gen::<bool>() { 1.0 } else { -1.0 };
            env.push(self.pos);
        }
    }

    fn run(seed: u64, act: Activation) -> Vec<f64> {
        let mut s = AgentScheduler::new(Vec::new(), 1.0, seed, act);
        for _ in 0..5 {
            s.add(Walker { pos: 0.0 });
        }
        s.run(200, &ExecutionControl::default()).unwrap();
        assert_eq!(s.steps(), 200);
        s.env
    }

    #[test]
    fn same_seed_same_trace_different_seed_differs() {
        assert_eq!(
            run(1, Activation::RandomOrder),
            run(1, Activation::RandomOrder)
        );
        assert_ne!(
            run(1, Activation::RandomOrder),
            run(2, Activation::RandomOrder)
        );
        assert_eq!(
            run(7, Activation::Sequential),
            run(7, Activation::Sequential)
        );
        assert_eq!(run(1, Activation::Sequential).len(), 1000);
    }

    #[test]
    fn cancellation_stops_run() {
        let mut s = AgentScheduler::new(Vec::new(), 1.0, 0, Activation::Sequential);
        s.add(Walker { pos: 0.0 });
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        assert!(s.run(10, &ctl).is_err());
        assert_eq!(s.steps(), 0);
    }
}
