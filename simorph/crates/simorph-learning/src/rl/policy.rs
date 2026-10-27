//! Stochastic policies on top of [`Mlp`].

use super::env::{Action, ActionSpace};
use crate::mlp::{softmax, Activation, Gradients, Mlp, Tape};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand_distr::StandardNormal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Policy {
    /// Softmax over logits produced by the network.
    Categorical { net: Mlp },
    /// Diagonal Gaussian: mean from the network, state-independent log std.
    Gaussian {
        net: Mlp,
        log_std: Vec<f64>,
        low: Vec<f64>,
        high: Vec<f64>,
    },
}

/// Gradient of a scalar objective w.r.t. the policy parameters.
#[derive(Debug, Clone)]
pub struct PolicyGrad {
    pub net: Gradients,
    pub log_std: Vec<f64>,
}

impl Policy {
    pub fn new(obs_dim: usize, space: &ActionSpace, hidden: &[usize], seed: u64) -> Self {
        let mut sizes = vec![obs_dim];
        sizes.extend_from_slice(hidden);
        match space {
            ActionSpace::Discrete(n) => {
                sizes.push(*n);
                let mut net = Mlp::new(&sizes, Activation::Tanh, Activation::Identity, seed);
                scale_last_layer(&mut net, 0.01);
                Policy::Categorical { net }
            }
            ActionSpace::Continuous { low, high } => {
                sizes.push(low.len());
                let mut net = Mlp::new(&sizes, Activation::Tanh, Activation::Identity, seed);
                scale_last_layer(&mut net, 0.01);
                Policy::Gaussian {
                    net,
                    log_std: vec![0.0; low.len()],
                    low: low.clone(),
                    high: high.clone(),
                }
            }
        }
    }

    pub fn net(&self) -> &Mlp {
        match self {
            Policy::Categorical { net } | Policy::Gaussian { net, .. } => net,
        }
    }
    pub fn net_mut(&mut self) -> &mut Mlp {
        match self {
            Policy::Categorical { net } | Policy::Gaussian { net, .. } => net,
        }
    }

    pub fn zero_grad(&self) -> PolicyGrad {
        PolicyGrad {
            net: Gradients::zeros(self.net()),
            log_std: match self {
                Policy::Gaussian { log_std, .. } => vec![0.0; log_std.len()],
                _ => vec![],
            },
        }
    }

    /// Sample an action; returns (env action, raw action vector used for log-prob, log-prob).
    pub fn sample(&self, obs: &[f64], rng: &mut ChaCha8Rng) -> (Action, Vec<f64>, f64) {
        let out = self.net().forward_raw(obs);
        match self {
            Policy::Categorical { .. } => {
                let p = softmax(&out);
                let u: f64 = rng.gen();
                let mut acc = 0.0;
                let mut a = p.len() - 1;
                for (i, pi) in p.iter().enumerate() {
                    acc += pi;
                    if u < acc {
                        a = i;
                        break;
                    }
                }
                (Action::Discrete(a), vec![a as f64], p[a].max(1e-300).ln())
            }
            Policy::Gaussian {
                log_std, low, high, ..
            } => {
                let raw: Vec<f64> = out
                    .iter()
                    .zip(log_std)
                    .map(|(m, s)| m + s.exp() * rng.sample::<f64, _>(StandardNormal))
                    .collect();
                let lp = gaussian_logp(&raw, &out, log_std);
                let clipped = raw
                    .iter()
                    .zip(low.iter().zip(high))
                    .map(|(v, (l, h))| v.clamp(*l, *h))
                    .collect();
                (Action::Continuous(clipped), raw, lp)
            }
        }
    }

    /// Deterministic action (mode).
    pub fn act_greedy(&self, obs: &[f64]) -> Action {
        let out = self.net().forward_raw(obs);
        match self {
            Policy::Categorical { .. } => Action::Discrete(
                out.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                    .map(|(i, _)| i)
                    .unwrap_or(0),
            ),
            Policy::Gaussian { low, high, .. } => Action::Continuous(
                out.iter()
                    .zip(low.iter().zip(high))
                    .map(|(v, (l, h))| v.clamp(*l, *h))
                    .collect(),
            ),
        }
    }

    /// Log-probability of `raw` at `obs`, entropy, and accumulation of
    /// `coef_logp * ∂logπ/∂θ + coef_ent * ∂H/∂θ` into `g`.
    pub fn logp_entropy_backward(
        &self,
        obs: &[f64],
        raw: &[f64],
        coef_logp: f64,
        coef_ent: f64,
        g: &mut PolicyGrad,
        tape: &mut Tape,
    ) -> (f64, f64) {
        let net = self.net();
        let out = net.forward_tape(obs, tape);
        match self {
            Policy::Categorical { .. } => {
                let a = raw[0] as usize;
                let p = softmax(&out);
                let logp = p[a].max(1e-300).ln();
                let h: f64 = -p.iter().map(|pi| pi * pi.max(1e-300).ln()).sum::<f64>();
                let d: Vec<f64> = (0..p.len())
                    .map(|j| {
                        let dlogp = if j == a { 1.0 } else { 0.0 } - p[j];
                        let dh = -p[j] * (p[j].max(1e-300).ln() + h);
                        coef_logp * dlogp + coef_ent * dh
                    })
                    .collect();
                net.backward(tape, &d, &mut g.net);
                (logp, h)
            }
            Policy::Gaussian { log_std, .. } => {
                let logp = gaussian_logp(raw, &out, log_std);
                let h: f64 = log_std
                    .iter()
                    .map(|s| s + 0.5 * (2.0 * std::f64::consts::PI * std::f64::consts::E).ln())
                    .sum();
                let d: Vec<f64> = out
                    .iter()
                    .zip(raw)
                    .zip(log_std)
                    .map(|((m, a), s)| coef_logp * (a - m) / (2.0 * s).exp())
                    .collect();
                net.backward(tape, &d, &mut g.net);
                for (k, (m, a)) in out.iter().zip(raw).enumerate() {
                    let z2 = ((a - m) / log_std[k].exp()).powi(2);
                    g.log_std[k] += coef_logp * (z2 - 1.0) + coef_ent;
                }
                (logp, h)
            }
        }
    }
}

fn scale_last_layer(net: &mut Mlp, s: f64) {
    if let Some(l) = net.layers.last_mut() {
        for w in &mut l.w {
            *w *= s;
        }
    }
}

pub fn gaussian_logp(a: &[f64], mean: &[f64], log_std: &[f64]) -> f64 {
    a.iter()
        .zip(mean)
        .zip(log_std)
        .map(|((a, m), s)| {
            -0.5 * ((a - m) / s.exp()).powi(2) - s - 0.5 * (2.0 * std::f64::consts::PI).ln()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// d logπ / d θ by backprop vs finite differences, both policy types.
    #[test]
    fn logprob_gradients_match_finite_differences() {
        for space in [
            ActionSpace::Discrete(3),
            ActionSpace::Continuous {
                low: vec![-2.0, -2.0],
                high: vec![2.0, 2.0],
            },
        ] {
            let mut pol = Policy::new(3, &space, &[6], 4);
            if let Policy::Gaussian { log_std, .. } = &mut pol {
                log_std[0] = -0.3;
                log_std[1] = 0.2;
            }
            // larger last layer so gradients are not tiny
            for w in &mut pol.net_mut().layers.last_mut().unwrap().w {
                *w *= 50.0;
            }
            let obs = [0.2, -0.4, 0.9];
            let raw = match space {
                ActionSpace::Discrete(_) => vec![2.0],
                _ => vec![0.5, -0.7],
            };
            let mut g = pol.zero_grad();
            let mut tape = Tape::default();
            let (lp, _) = pol.logp_entropy_backward(&obs, &raw, 1.0, 0.0, &mut g, &mut tape);
            let f = |p: &Policy| -> f64 {
                let out = p.net().forward_raw(&obs);
                match p {
                    Policy::Categorical { .. } => softmax(&out)[2].ln(),
                    Policy::Gaussian { log_std, .. } => gaussian_logp(&raw, &out, log_std),
                }
            };
            assert!((f(&pol) - lp).abs() < 1e-12);
            let p0 = pol.net().parameters();
            let an: Vec<f64> = g
                .net
                .w
                .iter()
                .zip(&g.net.b)
                .flat_map(|(w, b)| w.iter().chain(b.iter()).copied())
                .collect();
            for k in (0..p0.len()).step_by(3) {
                let mut q = pol.clone();
                let mut p = p0.clone();
                p[k] += 1e-6;
                q.net_mut().set_parameters(&p);
                let up = f(&q);
                p[k] -= 2e-6;
                q.net_mut().set_parameters(&p);
                let dn = f(&q);
                let num = (up - dn) / 2e-6;
                assert!(
                    (num - an[k]).abs() <= 1e-5 * num.abs().max(an[k].abs()).max(1e-6),
                    "param {k}: {num} vs {}",
                    an[k]
                );
            }
            if let Policy::Gaussian { .. } = pol {
                for k in 0..2 {
                    let mut q = pol.clone();
                    if let Policy::Gaussian { log_std, .. } = &mut q {
                        log_std[k] += 1e-6;
                    }
                    let up = f(&q);
                    if let Policy::Gaussian { log_std, .. } = &mut q {
                        log_std[k] -= 2e-6;
                    }
                    let num = (up - f(&q)) / 2e-6;
                    assert!(
                        (num - g.log_std[k]).abs() < 1e-5,
                        "log_std {k}: {num} vs {}",
                        g.log_std[k]
                    );
                }
            }
        }
    }
}
