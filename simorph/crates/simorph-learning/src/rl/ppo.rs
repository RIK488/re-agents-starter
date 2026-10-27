//! Proximal Policy Optimisation (Schulman et al. 2017) with GAE(λ).

use super::env::{Action, Environment};
use super::policy::{Policy, PolicyGrad};
use crate::mlp::{Activation, Gradients, Mlp, Optimizer, OptimizerConfig, Tape};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PpoConfig {
    pub total_steps: usize,
    pub rollout_steps: usize,
    pub epochs: usize,
    pub minibatch: usize,
    pub gamma: f64,
    pub lambda: f64,
    pub clip: f64,
    pub lr: f64,
    pub value_lr: f64,
    pub ent_coef: f64,
    pub hidden: Vec<usize>,
    pub max_grad_norm: f64,
    pub seed: u64,
}

impl Default for PpoConfig {
    fn default() -> Self {
        Self {
            total_steps: 50_000,
            rollout_steps: 1024,
            epochs: 6,
            minibatch: 128,
            gamma: 0.99,
            lambda: 0.95,
            clip: 0.2,
            lr: 1e-3,
            value_lr: 2e-3,
            ent_coef: 0.0,
            hidden: vec![32, 32],
            max_grad_norm: 0.5,
            seed: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RlReport {
    pub algorithm: String,
    pub environment: String,
    pub steps: usize,
    pub episode_returns: Vec<f64>,
    /// Mean return of the first 10 completed episodes (initial policy).
    pub initial_mean_return: f64,
    /// Mean return of the deterministic policy over evaluation episodes.
    pub eval_mean_return: f64,
    pub eval_episodes: usize,
    pub wall_time_s: f64,
}

/// Run `episodes` evaluation episodes with the greedy policy.
pub fn evaluate_policy(
    policy: &Policy,
    env: &mut dyn Environment,
    episodes: usize,
    seed: u64,
) -> f64 {
    let mut total = 0.0;
    for e in 0..episodes {
        let mut obs = env.reset(seed.wrapping_add(1_000_000 + e as u64));
        loop {
            let s = env.step(&policy.act_greedy(&obs));
            total += s.reward;
            obs = s.observation;
            if s.terminated || s.truncated {
                break;
            }
        }
    }
    total / episodes.max(1) as f64
}

struct Sample {
    obs: Vec<f64>,
    raw: Vec<f64>,
    logp: f64,
    adv: f64,
    ret: f64,
}

pub fn ppo(
    env: &mut dyn Environment,
    cfg: &PpoConfig,
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
        cfg.seed.wrapping_add(17),
    );
    let mut popt = Optimizer::new(OptimizerConfig::adam(cfg.lr), policy.net());
    let mut vopt = Optimizer::new(OptimizerConfig::adam(cfg.value_lr), &value);
    let mut std_adam = (
        vec![0.0; policy.zero_grad().log_std.len()],
        vec![0.0; policy.zero_grad().log_std.len()],
        0i32,
    );
    let mut episode = 0u64;
    let mut obs = env.reset(cfg.seed.wrapping_mul(7919).wrapping_add(episode));
    let mut ep_ret = 0.0;
    let mut returns = vec![];
    let mut steps = 0;
    let mut tape = Tape::default();
    while steps < cfg.total_steps {
        ctl.check()?;
        // ---- rollout
        let mut traj_obs = vec![];
        let mut traj_raw = vec![];
        let mut traj_logp = vec![];
        let mut traj_rew = vec![];
        let mut traj_val = vec![];
        let mut traj_next_val = vec![];
        let mut traj_end = vec![]; // true when the episode ended after this step (terminated or truncated)
        for _ in 0..cfg.rollout_steps {
            let v = value.forward_raw(&obs)[0];
            let (action, raw, logp) = policy.sample(&obs, &mut rng);
            let s = env.step(&action);
            steps += 1;
            ep_ret += s.reward;
            traj_obs.push(obs.clone());
            traj_raw.push(raw);
            traj_logp.push(logp);
            traj_rew.push(s.reward);
            traj_val.push(v);
            // bootstrap through truncation, not through termination
            let nv = if s.terminated {
                0.0
            } else {
                value.forward_raw(&s.observation)[0]
            };
            traj_next_val.push(nv);
            let end = s.terminated || s.truncated;
            traj_end.push(end);
            if end {
                returns.push(ep_ret);
                ep_ret = 0.0;
                episode += 1;
                obs = env.reset(cfg.seed.wrapping_mul(7919).wrapping_add(episode));
            } else {
                obs = s.observation;
            }
        }
        // ---- GAE
        let n = traj_rew.len();
        let mut adv = vec![0.0; n];
        let mut last = 0.0;
        for t in (0..n).rev() {
            let delta = traj_rew[t] + cfg.gamma * traj_next_val[t] - traj_val[t];
            let cont = if traj_end[t] || t == n - 1 { 0.0 } else { 1.0 };
            last = delta + cfg.gamma * cfg.lambda * cont * last;
            adv[t] = last;
        }
        let rets: Vec<f64> = adv.iter().zip(&traj_val).map(|(a, v)| a + v).collect();
        let mean = adv.iter().sum::<f64>() / n as f64;
        let sd = (adv.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / n as f64)
            .sqrt()
            .max(1e-8);
        let samples: Vec<Sample> = (0..n)
            .map(|t| Sample {
                obs: traj_obs[t].clone(),
                raw: traj_raw[t].clone(),
                logp: traj_logp[t],
                adv: (adv[t] - mean) / sd,
                ret: rets[t],
            })
            .collect();
        // ---- optimisation
        let mut order: Vec<usize> = (0..n).collect();
        for _ in 0..cfg.epochs {
            use rand::seq::SliceRandom;
            order.shuffle(&mut rng);
            for mb in order.chunks(cfg.minibatch) {
                let mut pg: PolicyGrad = policy.zero_grad();
                let mut vg = Gradients::zeros(&value);
                for &i in mb {
                    let s = &samples[i];
                    // ratio at current parameters (cheap forward first)
                    let mut tmp = policy.zero_grad();
                    let (logp_now, _) =
                        policy.logp_entropy_backward(&s.obs, &s.raw, 0.0, 0.0, &mut tmp, &mut tape);
                    let ratio = (logp_now - s.logp).exp();
                    let unclipped = ratio * s.adv;
                    let clipped = ratio.clamp(1.0 - cfg.clip, 1.0 + cfg.clip) * s.adv;
                    // loss = -min(...) - ent_coef H; gradient of -ratio*A w.r.t. logπ is -ratio*A when unclipped branch active
                    let coef = if unclipped <= clipped {
                        -ratio * s.adv
                    } else {
                        0.0
                    };
                    policy.logp_entropy_backward(
                        &s.obs,
                        &s.raw,
                        coef,
                        -cfg.ent_coef,
                        &mut pg,
                        &mut tape,
                    );
                    let vout = value.forward_tape(&s.obs, &mut tape);
                    value.backward(&tape, &[2.0 * (vout[0] - s.ret)], &mut vg);
                }
                let k = 1.0 / mb.len() as f64;
                pg.net.scale(k);
                pg.net.clip(cfg.max_grad_norm);
                popt.step(policy.net_mut(), &pg.net);
                vg.scale(k);
                vg.clip(cfg.max_grad_norm * 10.0);
                vopt.step(&mut value, &vg);
                if let Policy::Gaussian { log_std, .. } = &mut policy {
                    std_adam.2 += 1;
                    for j in 0..log_std.len() {
                        let gj = pg.log_std[j] * k;
                        std_adam.0[j] = 0.9 * std_adam.0[j] + 0.1 * gj;
                        std_adam.1[j] = 0.999 * std_adam.1[j] + 0.001 * gj * gj;
                        let mh = std_adam.0[j] / (1.0 - 0.9f64.powi(std_adam.2));
                        let vh = std_adam.1[j] / (1.0 - 0.999f64.powi(std_adam.2));
                        log_std[j] =
                            (log_std[j] - cfg.lr * mh / (vh.sqrt() + 1e-8)).clamp(-3.0, 1.0);
                    }
                }
            }
        }
    }
    let initial = returns.iter().take(10).sum::<f64>() / returns.len().clamp(1, 10) as f64;
    let eval = evaluate_policy(&policy, env, 10, cfg.seed);
    let _ = Action::Discrete(0);
    Ok((
        policy,
        RlReport {
            algorithm: "ppo".into(),
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
    use crate::rl::env::{CartPole, Pendulum};

    #[test]
    fn ppo_learns_cartpole() {
        let mut env = CartPole::new();
        let cfg = PpoConfig {
            total_steps: 40_960,
            seed: 1,
            ..Default::default()
        };
        let (_, r) = ppo(&mut env, &cfg, &ExecutionControl::default()).unwrap();
        eprintln!(
            "PPO CartPole: initial {:.1}, eval {:.1} ({} steps, {:.1} s)",
            r.initial_mean_return, r.eval_mean_return, r.steps, r.wall_time_s
        );
        assert!(r.eval_mean_return >= 3.0 * r.initial_mean_return, "{r:?}");
        assert!(r.eval_mean_return >= 195.0, "eval {}", r.eval_mean_return);
    }

    /// Long run on Pendulum (continuous Gaussian policy). Run with
    /// `cargo test -p simorph-learning --release -- --ignored ppo_pendulum`.
    #[test]
    #[ignore]
    fn ppo_pendulum_improves() {
        let mut env = Pendulum::new();
        let cfg = PpoConfig {
            total_steps: 200_000,
            rollout_steps: 2048,
            hidden: vec![64, 64],
            lr: 3e-4,
            value_lr: 1e-3,
            gamma: 0.95,
            seed: 0,
            ..Default::default()
        };
        let (_, r) = ppo(&mut env, &cfg, &ExecutionControl::default()).unwrap();
        eprintln!(
            "PPO Pendulum: initial {:.1}, eval {:.1} ({:.1} s)",
            r.initial_mean_return, r.eval_mean_return, r.wall_time_s
        );
        assert!(r.eval_mean_return > r.initial_mean_return + 300.0);
    }
}
