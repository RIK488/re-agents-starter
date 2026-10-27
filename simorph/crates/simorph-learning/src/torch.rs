//! Optional PyTorch backend.
//!
//! [`TorchMlpTrainer`] writes a self-contained Python script, runs it with the
//! interpreter selected by `simorph_adapter_python_bridge::python_interpreter()`
//! (isolated mode `-I`, cleared environment, timeout), and loads the trained
//! weights back into a native [`Mlp`].  The architecture, the standardisation
//! and the weight layout (row-major `out × in`, identical to `torch.nn.Linear`)
//! are shared, so the native model reproduces the PyTorch predictions.

use crate::mlp::{Activation, Layer, Mlp, Standardizer};
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TorchMlpTrainer {
    pub sizes: Vec<usize>,
    pub hidden: Activation,
    pub epochs: usize,
    pub lr: f64,
    pub batch_size: usize,
    pub seed: u64,
    pub timeout_s: f64,
}

const SCRIPT: &str = r#"
import json, sys
try:
    import torch
except Exception as e:
    print(json.dumps({"error": "torch unavailable: %s" % e})); sys.exit(0)
cfg = json.load(sys.stdin)
torch.manual_seed(cfg["seed"])
torch.set_default_dtype(torch.float64)
X = torch.tensor(cfg["x"]); Y = torch.tensor(cfg["y"])
xm, xs = X.mean(0), X.std(0, unbiased=False); xs[xs < 1e-12] = 1.0
ym, ys = Y.mean(0), Y.std(0, unbiased=False); ys[ys < 1e-12] = 1.0
Xn, Yn = (X - xm) / xs, (Y - ym) / ys
acts = {"tanh": torch.nn.Tanh, "relu": torch.nn.ReLU, "gelu": lambda: torch.nn.GELU(approximate="tanh")}
sizes = cfg["sizes"]; mods = []
for i in range(len(sizes) - 1):
    mods.append(torch.nn.Linear(sizes[i], sizes[i + 1]))
    if i + 2 < len(sizes):
        mods.append(acts[cfg["hidden"]]())
net = torch.nn.Sequential(*mods)
opt = torch.optim.Adam(net.parameters(), lr=cfg["lr"])
g = torch.Generator().manual_seed(cfg["seed"])
n = Xn.shape[0]
for epoch in range(cfg["epochs"]):
    perm = torch.randperm(n, generator=g)
    for k in range(0, n, cfg["batch_size"]):
        idx = perm[k:k + cfg["batch_size"]]
        opt.zero_grad()
        loss = ((net(Xn[idx]) - Yn[idx]) ** 2).mean()
        loss.backward(); opt.step()
with torch.no_grad():
    final = float(((net(Xn) - Yn) ** 2).mean())
layers = [{"w": m.weight.detach().reshape(-1).tolist(), "b": m.bias.detach().tolist()} for m in net if isinstance(m, torch.nn.Linear)]
print(json.dumps({"torch_version": torch.__version__, "final_train_loss": final, "layers": layers,
                  "x_mean": xm.tolist(), "x_std": xs.tolist(), "y_mean": ym.tolist(), "y_std": ys.tolist()}))
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TorchTrainingResult {
    pub torch_version: String,
    pub final_train_loss: f64,
    pub wall_time_s: f64,
}

impl TorchMlpTrainer {
    /// Whether PyTorch imports with the configured interpreter.
    pub fn available() -> std::result::Result<String, String> {
        simorph_adapter_python_bridge::python_module_available("torch")
    }

    pub fn train(&self, x: &[Vec<f64>], y: &[Vec<f64>]) -> Result<(Mlp, TorchTrainingResult)> {
        if let Err(reason) = Self::available() {
            return Err(SimorphError::NotImplemented(format!(
                "PyTorch backend unavailable: {reason}"
            )));
        }
        let hidden = match self.hidden {
            Activation::Tanh => "tanh",
            Activation::Relu => "relu",
            Activation::Gelu => "gelu",
            Activation::Identity => {
                return Err(SimorphError::InvalidInput(
                    "identity hidden activation not supported by the torch backend".into(),
                ))
            }
        };
        let cfg = serde_json::json!({"sizes": self.sizes, "hidden": hidden, "epochs": self.epochs, "lr": self.lr, "batch_size": self.batch_size, "seed": self.seed, "x": x, "y": y});
        let dir =
            std::env::temp_dir().join(format!("simorph-torch-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&dir)?;
        let script = dir.join("train_mlp.py");
        std::fs::write(&script, SCRIPT)?;
        let start = Instant::now();
        let mut child = Command::new(simorph_adapter_python_bridge::python_interpreter())
            .arg("-I")
            .arg(&script)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("OMP_NUM_THREADS", "1")
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .expect("piped")
            .write_all(cfg.to_string().as_bytes())?;
        let deadline = Duration::from_secs_f64(self.timeout_s);
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if start.elapsed() > deadline {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(SimorphError::Timeout(deadline.as_millis() as u64));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output()?;
        let _ = std::fs::remove_dir_all(&dir);
        let text = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(text.trim().lines().last().unwrap_or(""))
            .map_err(|e| SimorphError::Component {
            component: "torch-trainer".into(),
            message: format!(
                "bad output ({e}); stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            ),
        })?;
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return Err(SimorphError::NotImplemented(e.to_string()));
        }
        let vec_of = |k: &str| -> Vec<f64> {
            v[k].as_array()
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default()
        };
        let mut layers = vec![];
        for (i, l) in v["layers"]
            .as_array()
            .ok_or_else(|| SimorphError::Serialization("no layers".into()))?
            .iter()
            .enumerate()
        {
            let w: Vec<f64> = l["w"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x.as_f64())
                .collect();
            let b: Vec<f64> = l["b"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x.as_f64())
                .collect();
            let act = if i + 2 == self.sizes.len() {
                Activation::Identity
            } else {
                self.hidden
            };
            layers.push(Layer {
                n_in: self.sizes[i],
                n_out: self.sizes[i + 1],
                w,
                b,
                activation: act,
            });
        }
        let m = Mlp {
            layers,
            input_scaler: Standardizer {
                mean: vec_of("x_mean"),
                std: vec_of("x_std"),
            },
            output_scaler: Standardizer {
                mean: vec_of("y_mean"),
                std: vec_of("y_std"),
            },
        };
        m.check_shapes()?;
        Ok((
            m,
            TorchTrainingResult {
                torch_version: v["torch_version"].as_str().unwrap_or("?").into(),
                final_train_loss: v["final_train_loss"].as_f64().unwrap_or(f64::NAN),
                wall_time_s: start.elapsed().as_secs_f64(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torch_backend_trains_or_reports_unavailability() {
        let x: Vec<Vec<f64>> = (0..100)
            .map(|i| vec![-3.0 + 6.0 * i as f64 / 99.0])
            .collect();
        let y: Vec<Vec<f64>> = x.iter().map(|v| vec![v[0].sin()]).collect();
        let t = TorchMlpTrainer {
            sizes: vec![1, 32, 32, 1],
            hidden: Activation::Tanh,
            epochs: 300,
            lr: 5e-3,
            batch_size: 25,
            seed: 0,
            timeout_s: 120.0,
        };
        match TorchMlpTrainer::available() {
            Err(reason) => {
                eprintln!("SKIP torch backend: {reason}");
                assert!(matches!(
                    t.train(&x, &y),
                    Err(SimorphError::NotImplemented(_))
                ));
            }
            Ok(version) => {
                let (m, r) = t.train(&x, &y).unwrap();
                eprintln!(
                    "torch {version}: final loss {:.3e} in {:.1} s",
                    r.final_train_loss, r.wall_time_s
                );
                // the native model must reproduce torch's fit
                let e = crate::mlp::rmse(&m, &x, &y, 0);
                assert!(e < 0.05, "native evaluation of torch weights: RMSE {e}");
            }
        }
    }
}
