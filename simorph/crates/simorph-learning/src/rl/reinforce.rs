//! REINFORCE with a learned value baseline (Williams 1992).

use super::env::Environment;
use super::policy::Policy;
use super::ppo::{evaluate_policy, RlReport};
use crate::mlp::{Activation, Gradients, Mlp, Optimizer, OptimizerConfig, Tape};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReinforceConfig {
    pub episodes: usize,
    pub gamma: f64,
    pub lr: f64,
    pub value_lr: f64,
    pub hidden: Vec<usize>,
    pub seed: u64,
}

impl Default for ReinforceConfig {
    fn default() -> Self {
        Self {
            episodes: 300,
            gamma: 0.99,
            lr: 5e-3,
            value_lr: 1e-2,
            hidden: vec![32],
            seed: 0,
        }
    }
}

pub fn reinforce(
    env: &mut dyn Environment,
    cfg: &ReinforceConfig,
    ctl: &ExecutionControl,
) -> Result<(Policy, RlReport)> {
    let start = std::time::Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut policy = Policy::new(
        env.observation_dim(),
        &env.action_space(),
        &cfg.hidden,
        cfg.seed,
    );
    let mut vsizes = vec![env.observation_dim()];
    vsizes.extend_from_slice(&cfg.hidden);
    vsizes.push(1);
    let mut value = Mlp::new(
        &vsizes,
        Activation::Tanh,
        Activation::Identity,
        cfg.seed + 1,
    );
    let mut popt = Optimizer::new(OptimizerConfig::adam(cfg.lr), policy.net());
    let mut vopt = Optimizer::new(OptimizerConfig::adam(cfg.value_lr), &value);
    let mut returns = vec![];
    let mut steps = 0;
    let mut tape = Tape::default();
    for ep in 0..cfg.episodes {
        ctl.check()?;
        let mut obs = env.reset(cfg.seed.wrapping_mul(31).wrapping_add(ep as u64));
        let (mut os, mut rs, mut raws) = (vec![], vec![], vec![]);
        loop {
            let (a, raw, _) = policy.sample(&obs, &mut rng);
            let s = env.step(&a);
            steps += 1;
            os.push(obs.clone());
            raws.push(raw);
            rs.push(s.reward);
            obs = s.observation;
            if s.terminated || s.truncated {
                break;
            }
        }
        returns.push(rs.iter().sum());
        let mut g = 0.0;
        let mut gs = vec![0.0; rs.len()];
        for t in (0..rs.len()).rev() {
            g = rs[t] + cfg.gamma * g;
            gs[t] = g;
        }
        let mut pg = policy.zero_grad();
        let mut vg = Gradients::zeros(&value);
        for t in 0..rs.len() {
            let b = value.forward_tape(&os[t], &mut tape)[0];
            value.backward(&tape, &[2.0 * (b - gs[t])], &mut vg);
            // minimise -(G - b) log π
            policy.logp_entropy_backward(&os[t], &raws[t], -(gs[t] - b), 0.0, &mut pg, &mut tape);
        }
        let k = 1.0 / rs.len() as f64;
        pg.net.scale(k);
        pg.net.clip(1.0);
        vg.scale(k);
        vg.clip(10.0);
        popt.step(policy.net_mut(), &pg.net);
        vopt.step(&mut value, &vg);
    }
    let initial = returns.iter().take(10).sum::<f64>() / returns.len().clamp(1, 10) as f64;
    let eval = evaluate_policy(&policy, env, 10, cfg.seed);
    Ok((
        policy,
        RlReport {
            algorithm: "reinforce".into(),
            environment: env.name().into(),
            steps,
            episode_returns: returns,
            initial_mean_return: initial,
            eval_mean_return: eval,
            eval_episodes: 10,
            wall_time_s: start.elapsed().as_secs_f64(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::env::CartPole;

    #[test]
    fn reinforce_improves_cartpole() {
        let mut env = CartPole::new();
        let (_, r) = reinforce(
            &mut env,
            &ReinforceConfig {
                episodes: 250,
                seed: 2,
                ..Default::default()
            },
            &ExecutionControl::default(),
        )
        .unwrap();
        eprintln!(
            "REINFORCE CartPole: initial {:.1}, eval {:.1} ({:.1} s)",
            r.initial_mean_return, r.eval_mean_return, r.wall_time_s
        );
        assert!(r.eval_mean_return >= 3.0 * r.initial_mean_return, "{r:?}");
    }
}
