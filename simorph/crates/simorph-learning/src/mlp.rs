//! Dense multilayer perceptron with backpropagation.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Tanh,
    Relu,
    Gelu,
    Identity,
}

impl Activation {
    #[inline]
    pub fn f(self, z: f64) -> f64 {
        match self {
            Activation::Tanh => z.tanh(),
            Activation::Relu => z.max(0.0),
            Activation::Gelu => 0.5 * z * (1.0 + (GELU_C * (z + 0.044715 * z * z * z)).tanh()),
            Activation::Identity => z,
        }
    }
    /// Derivative with respect to the pre-activation `z` (`a = f(z)` is passed to avoid recomputation).
    #[inline]
    pub fn df(self, z: f64, a: f64) -> f64 {
        match self {
            Activation::Tanh => 1.0 - a * a,
            Activation::Relu => {
                if z > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Activation::Gelu => {
                let u = GELU_C * (z + 0.044715 * z * z * z);
                let t = u.tanh();
                let du = GELU_C * (1.0 + 3.0 * 0.044715 * z * z);
                0.5 * (1.0 + t) + 0.5 * z * (1.0 - t * t) * du
            }
            Activation::Identity => 1.0,
        }
    }
}

const GELU_C: f64 = 0.797_884_560_802_865_4; // sqrt(2/pi)

/// One dense layer: `y = act(W x + b)`, `W` stored row-major (`out × in`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub n_in: usize,
    pub n_out: usize,
    pub w: Vec<f64>,
    pub b: Vec<f64>,
    pub activation: Activation,
}

/// Per-feature affine standardisation `(x - mean) / std`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Standardizer {
    pub mean: Vec<f64>,
    pub std: Vec<f64>,
}

impl Standardizer {
    pub fn identity(n: usize) -> Self {
        Self {
            mean: vec![0.0; n],
            std: vec![1.0; n],
        }
    }
    pub fn fit(rows: &[Vec<f64>]) -> Self {
        let d = rows.first().map(|r| r.len()).unwrap_or(0);
        let n = rows.len().max(1) as f64;
        let mut mean = vec![0.0; d];
        for r in rows {
            for j in 0..d {
                mean[j] += r[j] / n;
            }
        }
        let mut var = vec![0.0; d];
        for r in rows {
            for j in 0..d {
                var[j] += (r[j] - mean[j]).powi(2) / n;
            }
        }
        let std = var
            .iter()
            .map(|v| if v.sqrt() > 1e-12 { v.sqrt() } else { 1.0 })
            .collect();
        Self { mean, std }
    }
    pub fn apply(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .zip(&self.mean)
            .zip(&self.std)
            .map(|((v, m), s)| (v - m) / s)
            .collect()
    }
    pub fn invert(&self, y: &[f64]) -> Vec<f64> {
        y.iter()
            .zip(&self.mean)
            .zip(&self.std)
            .map(|((v, m), s)| v * s + m)
            .collect()
    }
}

/// Multilayer perceptron with stored input/output standardisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mlp {
    pub layers: Vec<Layer>,
    pub input_scaler: Standardizer,
    pub output_scaler: Standardizer,
}

/// Activations recorded during a forward pass (for backpropagation).
#[derive(Debug, Clone, Default)]
pub struct Tape {
    /// `inputs[l]` is the input of layer `l`; the last entry is the network output.
    pub inputs: Vec<Vec<f64>>,
    pub pre: Vec<Vec<f64>>,
}

/// Gradients with the same shape as the network parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradients {
    pub w: Vec<Vec<f64>>,
    pub b: Vec<Vec<f64>>,
}

impl Gradients {
    pub fn zeros(m: &Mlp) -> Self {
        Self {
            w: m.layers.iter().map(|l| vec![0.0; l.w.len()]).collect(),
            b: m.layers.iter().map(|l| vec![0.0; l.b.len()]).collect(),
        }
    }
    pub fn scale(&mut self, s: f64) {
        for v in self.w.iter_mut().chain(self.b.iter_mut()) {
            for x in v.iter_mut() {
                *x *= s;
            }
        }
    }
    pub fn add(&mut self, o: &Gradients) {
        for (a, b) in self
            .w
            .iter_mut()
            .zip(&o.w)
            .chain(self.b.iter_mut().zip(&o.b))
        {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
    }
    pub fn norm(&self) -> f64 {
        self.w
            .iter()
            .chain(self.b.iter())
            .flatten()
            .map(|x| x * x)
            .sum::<f64>()
            .sqrt()
    }
    /// Clip to a maximum global L2 norm.
    pub fn clip(&mut self, max_norm: f64) {
        let n = self.norm();
        if n > max_norm && n > 0.0 {
            self.scale(max_norm / n);
        }
    }
}

impl Mlp {
    /// `sizes = [n_in, h1, ..., n_out]`; hidden layers use `hidden`, the output layer `output`.
    pub fn new(sizes: &[usize], hidden: Activation, output: Activation, seed: u64) -> Self {
        assert!(
            sizes.len() >= 2,
            "an MLP needs at least input and output sizes"
        );
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut layers = vec![];
        for i in 0..sizes.len() - 1 {
            let (n_in, n_out) = (sizes[i], sizes[i + 1]);
            let act = if i + 2 == sizes.len() { output } else { hidden };
            // He for ReLU/GELU, Xavier (Glorot normal) otherwise.
            let std = match act {
                Activation::Relu | Activation::Gelu => (2.0 / n_in as f64).sqrt(),
                _ => (2.0 / (n_in + n_out) as f64).sqrt(),
            };
            let normal = Normal::new(0.0, std).expect("valid std");
            let w = (0..n_in * n_out).map(|_| normal.sample(&mut rng)).collect();
            layers.push(Layer {
                n_in,
                n_out,
                w,
                b: vec![0.0; n_out],
                activation: act,
            });
        }
        Self {
            input_scaler: Standardizer::identity(sizes[0]),
            output_scaler: Standardizer::identity(*sizes.last().unwrap()),
            layers,
        }
    }

    pub fn n_inputs(&self) -> usize {
        self.layers[0].n_in
    }
    pub fn n_outputs(&self) -> usize {
        self.layers.last().unwrap().n_out
    }
    pub fn n_parameters(&self) -> usize {
        self.layers.iter().map(|l| l.w.len() + l.b.len()).sum()
    }

    /// Forward pass in *normalised* space, recording the tape.
    pub fn forward_tape(&self, x: &[f64], tape: &mut Tape) -> Vec<f64> {
        tape.inputs.clear();
        tape.pre.clear();
        let mut a = x.to_vec();
        for l in &self.layers {
            let mut z = l.b.clone();
            for o in 0..l.n_out {
                let row = &l.w[o * l.n_in..(o + 1) * l.n_in];
                let mut s = 0.0;
                for (wi, xi) in row.iter().zip(&a) {
                    s += wi * xi;
                }
                z[o] += s;
            }
            let out: Vec<f64> = z.iter().map(|&v| l.activation.f(v)).collect();
            tape.inputs.push(std::mem::replace(&mut a, out));
            tape.pre.push(z);
        }
        tape.inputs.push(a.clone());
        a
    }

    /// Forward pass in normalised space (no tape).
    pub fn forward_raw(&self, x: &[f64]) -> Vec<f64> {
        let mut a = x.to_vec();
        for l in &self.layers {
            let mut out = Vec::with_capacity(l.n_out);
            for o in 0..l.n_out {
                let row = &l.w[o * l.n_in..(o + 1) * l.n_in];
                let mut s = l.b[o];
                for (wi, xi) in row.iter().zip(&a) {
                    s += wi * xi;
                }
                out.push(l.activation.f(s));
            }
            a = out;
        }
        a
    }

    /// Prediction in physical (un-normalised) units.
    pub fn predict(&self, x: &[f64]) -> Vec<f64> {
        self.output_scaler
            .invert(&self.forward_raw(&self.input_scaler.apply(x)))
    }

    /// Backpropagate `d_out = ∂L/∂output` (normalised space) through the tape,
    /// accumulating into `g`.  Returns ∂L/∂input.
    pub fn backward(&self, tape: &Tape, d_out: &[f64], g: &mut Gradients) -> Vec<f64> {
        let mut delta: Vec<f64> = d_out.to_vec();
        for li in (0..self.layers.len()).rev() {
            let l = &self.layers[li];
            let z = &tape.pre[li];
            let a = &tape.inputs[li + 1];
            for o in 0..l.n_out {
                delta[o] *= l.activation.df(z[o], a[o]);
            }
            let x = &tape.inputs[li];
            let gw = &mut g.w[li];
            for o in 0..l.n_out {
                let d = delta[o];
                if d != 0.0 {
                    let row = &mut gw[o * l.n_in..(o + 1) * l.n_in];
                    for (gi, xi) in row.iter_mut().zip(x) {
                        *gi += d * xi;
                    }
                }
                g.b[li][o] += d;
            }
            let mut prev = vec![0.0; l.n_in];
            for o in 0..l.n_out {
                let d = delta[o];
                if d != 0.0 {
                    let row = &l.w[o * l.n_in..(o + 1) * l.n_in];
                    for (pi, wi) in prev.iter_mut().zip(row) {
                        *pi += d * wi;
                    }
                }
            }
            delta = prev;
        }
        delta
    }

    /// Flattened parameter vector (for checks and hashing).
    pub fn parameters(&self) -> Vec<f64> {
        self.layers
            .iter()
            .flat_map(|l| l.w.iter().chain(l.b.iter()).copied())
            .collect()
    }
    pub fn set_parameters(&mut self, p: &[f64]) {
        let mut k = 0;
        for l in &mut self.layers {
            for v in l.w.iter_mut().chain(l.b.iter_mut()) {
                *v = p[k];
                k += 1;
            }
        }
    }

    /// SHA-256 of the JSON encoding (weights, architecture, scalers).
    pub fn content_hash(&self) -> String {
        simorph_core::hashing::hash_json(self).expect("serialisable")
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("serialisable")
    }
    pub fn from_json(s: &str) -> Result<Self> {
        let m: Mlp = serde_json::from_str(s)?;
        m.check_shapes()?;
        Ok(m)
    }
    pub fn check_shapes(&self) -> Result<()> {
        for (i, l) in self.layers.iter().enumerate() {
            if l.w.len() != l.n_in * l.n_out || l.b.len() != l.n_out {
                return Err(SimorphError::InvalidInput(format!(
                    "layer {i} has inconsistent shapes"
                )));
            }
            if i > 0 && self.layers[i - 1].n_out != l.n_in {
                return Err(SimorphError::InvalidInput(format!(
                    "layer {i} input size does not match previous output"
                )));
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ losses

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loss {
    /// Mean squared error (regression), in normalised output space.
    Mse,
    /// Softmax cross-entropy; targets are one-hot (or class probabilities).
    SoftmaxCrossEntropy,
}

pub fn softmax(z: &[f64]) -> Vec<f64> {
    let m = z.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = z.iter().map(|v| (v - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|v| v / s).collect()
}

impl Loss {
    /// Loss value and gradient w.r.t. the network output.
    pub fn value_grad(self, out: &[f64], target: &[f64]) -> (f64, Vec<f64>) {
        match self {
            Loss::Mse => {
                let n = out.len() as f64;
                let mut l = 0.0;
                let g = out
                    .iter()
                    .zip(target)
                    .map(|(o, t)| {
                        l += (o - t).powi(2) / n;
                        2.0 * (o - t) / n
                    })
                    .collect();
                (l, g)
            }
            Loss::SoftmaxCrossEntropy => {
                let p = softmax(out);
                let l = -target
                    .iter()
                    .zip(&p)
                    .map(|(t, pi)| t * pi.max(1e-300).ln())
                    .sum::<f64>();
                (l, p.iter().zip(target).map(|(pi, t)| pi - t).collect())
            }
        }
    }
}

// ------------------------------------------------------------------ optimisers

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptimizerConfig {
    Sgd {
        lr: f64,
        momentum: f64,
    },
    Adam {
        lr: f64,
        beta1: f64,
        beta2: f64,
        eps: f64,
    },
}

impl OptimizerConfig {
    pub fn adam(lr: f64) -> Self {
        OptimizerConfig::Adam {
            lr,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
        }
    }
}

/// Optimiser state.
#[derive(Debug, Clone)]
pub struct Optimizer {
    pub config: OptimizerConfig,
    m: Gradients,
    v: Gradients,
    t: u64,
}

impl Optimizer {
    pub fn new(config: OptimizerConfig, model: &Mlp) -> Self {
        Self {
            config,
            m: Gradients::zeros(model),
            v: Gradients::zeros(model),
            t: 0,
        }
    }
    /// Apply one update with gradient `g` (already averaged over the batch).
    pub fn step(&mut self, model: &mut Mlp, g: &Gradients) {
        self.t += 1;
        match self.config {
            OptimizerConfig::Sgd { lr, momentum } => {
                for li in 0..model.layers.len() {
                    for (k, p) in model.layers[li].w.iter_mut().enumerate() {
                        let m = &mut self.m.w[li][k];
                        *m = momentum * *m + g.w[li][k];
                        *p -= lr * *m;
                    }
                    for (k, p) in model.layers[li].b.iter_mut().enumerate() {
                        let m = &mut self.m.b[li][k];
                        *m = momentum * *m + g.b[li][k];
                        *p -= lr * *m;
                    }
                }
            }
            OptimizerConfig::Adam {
                lr,
                beta1,
                beta2,
                eps,
            } => {
                let bc1 = 1.0 - beta1.powi(self.t as i32);
                let bc2 = 1.0 - beta2.powi(self.t as i32);
                let upd = |p: &mut f64, gr: f64, m: &mut f64, v: &mut f64| {
                    *m = beta1 * *m + (1.0 - beta1) * gr;
                    *v = beta2 * *v + (1.0 - beta2) * gr * gr;
                    *p -= lr * (*m / bc1) / ((*v / bc2).sqrt() + eps);
                };
                for li in 0..model.layers.len() {
                    for k in 0..model.layers[li].w.len() {
                        upd(
                            &mut model.layers[li].w[k],
                            g.w[li][k],
                            &mut self.m.w[li][k],
                            &mut self.v.w[li][k],
                        );
                    }
                    for k in 0..model.layers[li].b.len() {
                        upd(
                            &mut model.layers[li].b[k],
                            g.b[li][k],
                            &mut self.m.b[li][k],
                            &mut self.v.b[li][k],
                        );
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ training

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainConfig {
    pub loss: Loss,
    pub optimizer: OptimizerConfig,
    pub epochs: usize,
    pub batch_size: usize,
    /// Fraction of the data held out for early stopping (0 = none).
    pub validation_fraction: f64,
    /// Stop when the validation loss has not improved for this many epochs (0 = never).
    pub patience: usize,
    /// Fit input/output standardisation on the training data.
    pub standardize: bool,
    pub seed: u64,
    /// Optional global gradient-norm clipping.
    pub clip_norm: Option<f64>,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            loss: Loss::Mse,
            optimizer: OptimizerConfig::adam(1e-2),
            epochs: 500,
            batch_size: 32,
            validation_fraction: 0.0,
            patience: 0,
            standardize: true,
            seed: 0,
            clip_norm: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainReport {
    pub epochs_run: usize,
    pub final_train_loss: f64,
    pub best_validation_loss: Option<f64>,
    pub stopped_early: bool,
    pub train_loss_history: Vec<f64>,
    pub n_train: usize,
    pub n_validation: usize,
    pub model_hash: String,
}

/// Train `model` on `(x, y)` in physical units.  Deterministic for a seed.
pub fn train(
    model: &mut Mlp,
    x: &[Vec<f64>],
    y: &[Vec<f64>],
    cfg: &TrainConfig,
    ctl: &ExecutionControl,
) -> Result<TrainReport> {
    if x.len() != y.len() || x.is_empty() {
        return Err(SimorphError::InvalidInput(
            "training set empty or x/y length mismatch".into(),
        ));
    }
    if x[0].len() != model.n_inputs() || y[0].len() != model.n_outputs() {
        return Err(SimorphError::InvalidInput(format!(
            "data shape ({}, {}) does not match model ({}, {})",
            x[0].len(),
            y[0].len(),
            model.n_inputs(),
            model.n_outputs()
        )));
    }
    if x.iter()
        .flatten()
        .chain(y.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(SimorphError::InvalidInput(
            "training data contain non-finite values".into(),
        ));
    }
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut idx: Vec<usize> = (0..x.len()).collect();
    // deterministic shuffle, then split
    for i in (1..idx.len()).rev() {
        let j = rng.gen_range(0..=i);
        idx.swap(i, j);
    }
    let n_val = ((x.len() as f64) * cfg.validation_fraction).round() as usize;
    let n_val = n_val.min(x.len().saturating_sub(1));
    let (val_idx, train_idx) = idx.split_at(n_val);
    let train_idx = train_idx.to_vec();
    if cfg.standardize {
        let xs: Vec<Vec<f64>> = train_idx.iter().map(|&i| x[i].clone()).collect();
        model.input_scaler = Standardizer::fit(&xs);
        if cfg.loss == Loss::Mse {
            let ys: Vec<Vec<f64>> = train_idx.iter().map(|&i| y[i].clone()).collect();
            model.output_scaler = Standardizer::fit(&ys);
        }
    }
    let xn: Vec<Vec<f64>> = x.iter().map(|r| model.input_scaler.apply(r)).collect();
    let yn: Vec<Vec<f64>> = if cfg.loss == Loss::Mse {
        y.iter().map(|r| model.output_scaler.apply(r)).collect()
    } else {
        y.to_vec()
    };
    let mut opt = Optimizer::new(cfg.optimizer, model);
    let mut tape = Tape::default();
    let mut history = vec![];
    let mut best_val = f64::INFINITY;
    let mut best_params: Option<Vec<f64>> = None;
    let mut since_best = 0;
    let mut stopped_early = false;
    let mut order = train_idx.clone();
    let mut epochs_run = 0;
    for _epoch in 0..cfg.epochs {
        ctl.check()?;
        epochs_run += 1;
        for i in (1..order.len()).rev() {
            let j = rng.gen_range(0..=i);
            order.swap(i, j);
        }
        let mut epoch_loss = 0.0;
        for batch in order.chunks(cfg.batch_size.max(1)) {
            let mut g = Gradients::zeros(model);
            for &i in batch {
                let out = model.forward_tape(&xn[i], &mut tape);
                let (l, d) = cfg.loss.value_grad(&out, &yn[i]);
                epoch_loss += l;
                model.backward(&tape, &d, &mut g);
            }
            g.scale(1.0 / batch.len() as f64);
            if let Some(c) = cfg.clip_norm {
                g.clip(c);
            }
            opt.step(model, &g);
        }
        history.push(epoch_loss / order.len() as f64);
        if !val_idx.is_empty() {
            let vl = val_idx
                .iter()
                .map(|&i| cfg.loss.value_grad(&model.forward_raw(&xn[i]), &yn[i]).0)
                .sum::<f64>()
                / val_idx.len() as f64;
            if vl < best_val - 1e-12 {
                best_val = vl;
                best_params = Some(model.parameters());
                since_best = 0;
            } else {
                since_best += 1;
                if cfg.patience > 0 && since_best >= cfg.patience {
                    stopped_early = true;
                    break;
                }
            }
        }
    }
    if let Some(p) = best_params {
        model.set_parameters(&p);
    }
    Ok(TrainReport {
        epochs_run,
        final_train_loss: *history.last().unwrap_or(&f64::NAN),
        best_validation_loss: if val_idx.is_empty() {
            None
        } else {
            Some(best_val)
        },
        stopped_early,
        train_loss_history: history,
        n_train: train_idx.len(),
        n_validation: val_idx.len(),
        model_hash: model.content_hash(),
    })
}

/// Root-mean-square error of the model on `(x, y)` (physical units, output `k`).
pub fn rmse(model: &Mlp, x: &[Vec<f64>], y: &[Vec<f64>], k: usize) -> f64 {
    let s: f64 = x
        .iter()
        .zip(y)
        .map(|(xi, yi)| (model.predict(xi)[k] - yi[k]).powi(2))
        .sum();
    (s / x.len().max(1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loss_of(m: &Mlp, x: &[f64], t: &[f64], loss: Loss) -> f64 {
        loss.value_grad(&m.forward_raw(x), t).0
    }

    #[test]
    fn gradient_check_all_activations_and_losses() {
        for (act, loss, n_out) in [
            (Activation::Tanh, Loss::Mse, 2),
            (Activation::Gelu, Loss::Mse, 2),
            (Activation::Tanh, Loss::SoftmaxCrossEntropy, 3),
            (Activation::Relu, Loss::Mse, 1),
        ] {
            let m = Mlp::new(&[3, 5, 4, n_out], act, Activation::Identity, 7);
            let x = [0.3, -0.7, 1.1];
            let t: Vec<f64> = if loss == Loss::Mse {
                (0..n_out).map(|i| 0.2 * i as f64 - 0.1).collect()
            } else {
                vec![0.0, 1.0, 0.0]
            };
            let mut tape = Tape::default();
            let out = m.forward_tape(&x, &mut tape);
            let (_, d) = loss.value_grad(&out, &t);
            let mut g = Gradients::zeros(&m);
            m.backward(&tape, &d, &mut g);
            let analytic: Vec<f64> =
                g.w.iter()
                    .zip(&g.b)
                    .flat_map(|(w, b)| w.iter().chain(b.iter()).copied())
                    .collect();
            let p0 = m.parameters();
            let mut worst: f64 = 0.0;
            for k in 0..p0.len() {
                let h = 1e-6;
                let mut mp = m.clone();
                let mut p = p0.clone();
                p[k] += h;
                mp.set_parameters(&p);
                let lp = loss_of(&mp, &x, &t, loss);
                p[k] -= 2.0 * h;
                mp.set_parameters(&p);
                let lm = loss_of(&mp, &x, &t, loss);
                let num = (lp - lm) / (2.0 * h);
                let denom = num.abs().max(analytic[k].abs()).max(1e-7);
                // ReLU kinks: skip parameters whose derivative is ~0 on both sides
                worst = worst.max((num - analytic[k]).abs() / denom);
            }
            assert!(
                worst < 1e-5,
                "{act:?}/{loss:?}: worst relative gradient error {worst}"
            );
        }
    }

    #[test]
    fn fits_sine_to_rmse_below_0_02() {
        let x: Vec<Vec<f64>> = (0..200)
            .map(|i| vec![-std::f64::consts::PI + 2.0 * std::f64::consts::PI * i as f64 / 199.0])
            .collect();
        let y: Vec<Vec<f64>> = x.iter().map(|v| vec![v[0].sin()]).collect();
        let mut m = Mlp::new(&[1, 32, 32, 1], Activation::Tanh, Activation::Identity, 1);
        let cfg = TrainConfig {
            epochs: 400,
            batch_size: 32,
            optimizer: OptimizerConfig::adam(5e-3),
            seed: 3,
            ..Default::default()
        };
        train(&mut m, &x, &y, &cfg, &ExecutionControl::default()).unwrap();
        // independent test grid (midpoints)
        let xt: Vec<Vec<f64>> = (0..199)
            .map(|i| {
                vec![-std::f64::consts::PI + 2.0 * std::f64::consts::PI * (i as f64 + 0.5) / 199.0]
            })
            .collect();
        let yt: Vec<Vec<f64>> = xt.iter().map(|v| vec![v[0].sin()]).collect();
        let e = rmse(&m, &xt, &yt, 0);
        assert!(e < 0.02, "sine RMSE {e}");
    }

    #[test]
    fn xor_is_learned_exactly() {
        let x = vec![
            vec![0.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 0.0],
            vec![1.0, 1.0],
        ];
        let y = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![0.0, 1.0],
            vec![1.0, 0.0],
        ];
        let mut m = Mlp::new(&[2, 8, 2], Activation::Tanh, Activation::Identity, 11);
        let cfg = TrainConfig {
            loss: Loss::SoftmaxCrossEntropy,
            epochs: 500,
            batch_size: 4,
            optimizer: OptimizerConfig::adam(0.05),
            standardize: false,
            seed: 1,
            ..Default::default()
        };
        train(&mut m, &x, &y, &cfg, &ExecutionControl::default()).unwrap();
        for (xi, yi) in x.iter().zip(&y) {
            let p = m.predict(xi);
            let cls = if p[0] > p[1] { 0 } else { 1 };
            let truth = if yi[0] > yi[1] { 0 } else { 1 };
            assert_eq!(cls, truth);
        }
    }

    #[test]
    fn sgd_momentum_also_converges_and_training_is_deterministic() {
        let x: Vec<Vec<f64>> = (0..50).map(|i| vec![i as f64 / 49.0]).collect();
        let y: Vec<Vec<f64>> = x.iter().map(|v| vec![3.0 * v[0] - 1.0]).collect();
        let cfg = TrainConfig {
            epochs: 200,
            optimizer: OptimizerConfig::Sgd {
                lr: 0.05,
                momentum: 0.9,
            },
            seed: 5,
            ..Default::default()
        };
        let run = || {
            let mut m = Mlp::new(&[1, 4, 1], Activation::Tanh, Activation::Identity, 2);
            train(&mut m, &x, &y, &cfg, &ExecutionControl::default()).unwrap();
            m
        };
        let a = run();
        let b = run();
        assert_eq!(a.content_hash(), b.content_hash());
        assert!(rmse(&a, &x, &y, 0) < 0.05);
        let c = Mlp::new(&[1, 4, 1], Activation::Tanh, Activation::Identity, 3);
        assert_ne!(a.content_hash(), c.content_hash());
    }

    #[test]
    fn early_stopping_and_serialisation_round_trip() {
        let x: Vec<Vec<f64>> = (0..60).map(|i| vec![i as f64 / 59.0]).collect();
        let y: Vec<Vec<f64>> = x.iter().map(|v| vec![v[0] * v[0]]).collect();
        let mut m = Mlp::new(&[1, 8, 1], Activation::Tanh, Activation::Identity, 4);
        let cfg = TrainConfig {
            epochs: 3000,
            validation_fraction: 0.25,
            patience: 20,
            seed: 9,
            ..Default::default()
        };
        let r = train(&mut m, &x, &y, &cfg, &ExecutionControl::default()).unwrap();
        assert!(
            r.stopped_early && r.epochs_run < 3000,
            "{} epochs",
            r.epochs_run
        );
        assert_eq!(r.n_validation, 15);
        let back = Mlp::from_json(&m.to_json()).unwrap();
        assert_eq!(back.content_hash(), m.content_hash());
        assert_eq!(back.predict(&[0.5]), m.predict(&[0.5]));
    }

    #[test]
    fn rejects_bad_data_and_honours_cancellation() {
        let mut m = Mlp::new(&[1, 2, 1], Activation::Tanh, Activation::Identity, 0);
        assert!(train(
            &mut m,
            &[vec![f64::NAN]],
            &[vec![1.0]],
            &TrainConfig::default(),
            &ExecutionControl::default()
        )
        .is_err());
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        assert!(matches!(
            train(
                &mut m,
                &[vec![1.0]],
                &[vec![1.0]],
                &TrainConfig::default(),
                &ctl
            ),
            Err(SimorphError::Cancelled)
        ));
    }
}
