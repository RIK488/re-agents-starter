//! Training jobs.

use crate::dataset::{materialise, sha256_file, Dataset, DatasetSpec};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_adapter_python_bridge::PythonComponent;
use simorph_core::*;
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use simorph_security::SandboxPolicy;
use simorph_svc_ai_laboratory::{Candidate, ComponentResolver};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrainerSpec {
    /// A registered component with the `Training` capability.
    InProcess { component: ComponentId },
    /// A Python script speaking the SIMORPH protocol, run through the bridge.
    Python {
        script: PathBuf,
        #[serde(default)]
        args: Vec<String>,
    },
    /// An external command speaking the SIMORPH protocol over stdio.
    External { command: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainingBudget {
    pub max_wall_time_s: f64,
    pub epochs: f64,
}

impl Default for TrainingBudget {
    fn default() -> Self {
        Self {
            max_wall_time_s: 600.0,
            epochs: 100.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainingJob {
    pub candidate: Candidate,
    pub trainer: TrainerSpec,
    pub data: DatasetSpec,
    /// Name of the candidate parameter the trainer reads and rewrites (e.g. `weights`).
    pub parameter: String,
    /// Extra trainer inputs (learning rate, …).
    #[serde(default)]
    pub trainer_inputs: IndexMap<String, serde_json::Value>,
    pub budget: TrainingBudget,
    pub sandbox: SandboxPolicy,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainingResult {
    pub new_candidate: Candidate,
    pub metrics: IndexMap<String, f64>,
    pub outputs: IndexMap<String, serde_json::Value>,
    pub dataset: Dataset,
    pub artifacts: Vec<Artifact>,
    pub provenance: Vec<RecordId>,
    pub wall_time_s: f64,
}

pub struct TrainingService {
    pub ledger: Ledger,
    pub work_dir: PathBuf,
}

impl TrainingService {
    pub fn new(ledger: Ledger, work_dir: impl Into<PathBuf>) -> Self {
        Self {
            ledger,
            work_dir: work_dir.into(),
        }
    }

    fn trainer(
        &self,
        spec: &TrainerSpec,
        resolver: &dyn ComponentResolver,
        sandbox: &SandboxPolicy,
    ) -> Result<Arc<dyn Component>> {
        match spec {
            TrainerSpec::InProcess { component } => {
                let c = resolver.resolve(*component).ok_or_else(|| {
                    SimorphError::NotFound(format!("trainer component {component}"))
                })?;
                if !c.manifest().has_capability(&Capability::Training) {
                    return Err(SimorphError::Incompatible(format!(
                        "component `{}` has no Training capability",
                        c.manifest().name
                    )));
                }
                Ok(c)
            }
            TrainerSpec::Python { script, args } => {
                let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                Ok(Arc::new(PythonComponent::from_script_with(
                    simorph_adapter_python_bridge::python_interpreter(),
                    script,
                    &a,
                    sandbox.clone(),
                )?))
            }
            TrainerSpec::External { command } => {
                let (prog, args) = command.split_first().ok_or_else(|| {
                    SimorphError::InvalidInput("empty external trainer command".into())
                })?;
                Ok(Arc::new(simorph_execution::SubprocessComponent::new(
                    prog.clone(),
                    args.to_vec(),
                    sandbox.clone(),
                )?))
            }
        }
    }

    /// Run a training job: materialise data, train, hash artifacts, record provenance.
    pub fn run(
        &self,
        job: &TrainingJob,
        resolver: &dyn ComponentResolver,
        ctl: &ExecutionControl,
    ) -> Result<TrainingResult> {
        let t0 = std::time::Instant::now();
        job.sandbox.validate()?;
        std::fs::create_dir_all(&self.work_dir)?;
        let mut prov = vec![];
        let job_rec = self.ledger.record(
            ProvenanceKind::Experiment,
            format!("entraînement de {}", job.candidate.name),
            job,
            &[],
        )?;
        prov.push(job_rec);
        // 1. data
        let data_component = match &job.data {
            DatasetSpec::Synthetic { component, .. } => resolver.resolve(*component),
            _ => None,
        };
        let dataset = materialise(
            &job.data,
            data_component.as_deref(),
            &self.work_dir.join("datasets"),
            &self.ledger,
            ctl,
        )?;
        if let Some(p) = dataset.provenance {
            prov.push(p);
        }
        // 2. train
        let trainer = self.trainer(&job.trainer, resolver, &job.sandbox)?;
        let mut req = TaskRequest::new(trainer.manifest().id).seed(job.seed);
        for (k, v) in &job.trainer_inputs {
            req.inputs.insert(k.clone(), v.clone());
        }
        req.inputs.insert(
            "dataset_path".into(),
            serde_json::Value::from(dataset.path.to_string_lossy().to_string()),
        );
        req.inputs
            .entry("epochs".into())
            .or_insert(serde_json::Value::from(job.budget.epochs));
        // Candidate parameters are visible to the trainer (the one named
        // `parameter` is the one it must rewrite).
        for (k, v) in &job.candidate.parameters {
            req.inputs.insert(k.clone(), v.clone());
        }
        // Only pass inputs the trainer declares (dataset_path is optional for in-process trainers).
        let declared: Vec<String> = trainer
            .manifest()
            .input_schema
            .iter()
            .map(|p| p.name.clone())
            .collect();
        req.inputs.retain(|k, _| declared.contains(k));
        let ctl2 = ExecutionControl {
            cancel: ctl.cancel.clone(),
            deadline: Some(Deadline::after(std::time::Duration::from_secs_f64(
                job.budget.max_wall_time_s,
            ))),
        };
        let res = run_component(trainer.as_ref(), &req, &ctl2)?;
        let new_value =
            res.outputs
                .get(&job.parameter)
                .cloned()
                .ok_or_else(|| SimorphError::Component {
                    component: trainer.manifest().name.clone(),
                    message: format!("trainer produced no `{}`", job.parameter),
                })?;
        let mut params = job.candidate.parameters.clone();
        params.insert(job.parameter.clone(), new_value);
        let new_candidate = job.candidate.child(
            &format!("{} +{}", job.candidate.name, trainer.manifest().name),
            params,
            &format!(
                "entraînement sur {} ({} lignes)",
                dataset.description, dataset.rows
            ),
        );
        // 3. artifacts: any string output that is an existing file
        let mut artifacts = vec![];
        for (k, v) in &res.outputs {
            if let Some(s) = v.as_str() {
                let p = Path::new(s);
                if p.is_file() {
                    artifacts.push(Artifact {
                        name: k.clone(),
                        path: p.to_path_buf(),
                        sha256: sha256_file(p)?,
                    });
                }
            }
        }
        let params_path = self
            .work_dir
            .join(format!("candidate-{}.json", new_candidate.id));
        std::fs::write(&params_path, serde_json::to_vec_pretty(&new_candidate)?)?;
        artifacts.push(Artifact {
            name: "candidate".into(),
            path: params_path.clone(),
            sha256: sha256_file(&params_path)?,
        });
        let run_rec = self.ledger.record(ProvenanceKind::Run, format!("run d'entraînement {}", res.run_id), &serde_json::json!({"run_id": res.run_id, "request_hash": res.request_hash, "output_hash": res.output_hash(), "metrics": res.metrics, "artifacts": artifacts}), &prov)?;
        prov.push(run_rec);
        let cand_rec = self.ledger.record(
            ProvenanceKind::Improvement,
            format!("candidat {}", new_candidate.name),
            &new_candidate,
            &[run_rec],
        )?;
        prov.push(cand_rec);
        Ok(TrainingResult {
            new_candidate,
            metrics: res.metrics.clone(),
            outputs: res.outputs.clone(),
            dataset,
            artifacts,
            provenance: prov,
            wall_time_s: t0.elapsed().as_secs_f64(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{render, TemplateKind};
    use serde_json::json;
    use simorph_experiment::{Design, Factor};
    use simorph_plugin_ai as ai;
    use simorph_svc_ai_laboratory::FnResolver;
    use std::process::Command;

    fn resolver() -> FnResolver<impl Fn(ComponentId) -> Option<Arc<dyn Component>> + Send + Sync> {
        FnResolver(|id: ComponentId| ai::build(id).map(Arc::from))
    }

    fn heldout_accuracy(weights: &serde_json::Value) -> f64 {
        let ev = ai::classifier::ClassifierEval::new();
        run_component(
            &ev,
            &TaskRequest::new(ev.manifest().id)
                .input("weights", weights.clone())
                .input("dataset_seed", 777)
                .input("n_samples", 1000),
            &ExecutionControl::default(),
        )
        .unwrap()
        .get_f64("accuracy")
        .unwrap()
    }

    #[test]
    fn in_process_training_improves_heldout_accuracy_with_synthetic_dataset_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::in_memory();
        let svc = TrainingService::new(ledger.clone(), dir.path());
        let eval_id = ai::classifier::ClassifierEval::new().manifest().id;
        let zero = json!([0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let job = TrainingJob {
            candidate: Candidate::new(
                "zero",
                [("weights".to_string(), zero.clone())]
                    .into_iter()
                    .collect(),
                "base",
            ),
            trainer: TrainerSpec::InProcess {
                component: ai::classifier::ClassifierTrain::new().manifest().id,
            },
            // synthetic data: evaluate the zero classifier on several dataset seeds (demonstrates the data step)
            data: DatasetSpec::Synthetic {
                component: eval_id,
                design: Design::latin_hypercube(
                    vec![Factor::new("dataset_seed", "1", 1.0, 50.0)],
                    4,
                    3,
                )
                .unwrap(),
                fixed_inputs: [
                    ("weights".to_string(), zero.clone()),
                    ("n_samples".to_string(), json!(100)),
                ]
                .into_iter()
                .collect(),
                seed: 1,
            },
            parameter: "weights".into(),
            trainer_inputs: [
                ("dataset_seed".to_string(), json!(1)),
                ("n_samples".to_string(), json!(1500)),
                ("learning_rate".to_string(), json!(0.5)),
            ]
            .into_iter()
            .collect(),
            budget: TrainingBudget {
                max_wall_time_s: 60.0,
                epochs: 200.0,
            },
            sandbox: SandboxPolicy::strict(),
            seed: 0,
        };
        let before = heldout_accuracy(&zero);
        let r = svc
            .run(&job, &resolver(), &ExecutionControl::default())
            .unwrap();
        let after = heldout_accuracy(&r.new_candidate.parameters["weights"]);
        assert!(after > before + 0.2, "before {before} after {after}");
        assert_eq!(r.dataset.rows, 4);
        assert!(r.dataset.path.is_file());
        assert_eq!(r.dataset.sha256, sha256_file(&r.dataset.path).unwrap());
        assert!(r.dataset.provenance.is_some());
        assert!(r.artifacts.iter().any(|a| a.name == "candidate"));
        assert!(r.provenance.len() >= 4);
        ledger.verify_chain().unwrap();
        assert_eq!(ledger.by_kind(ProvenanceKind::Dataset).len(), 1);
    }

    #[test]
    fn file_dataset_hash_mismatch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.jsonl");
        std::fs::write(&p, "{\"inputs\":{},\"outputs\":{}}\n").unwrap();
        let ledger = Ledger::in_memory();
        let bad = DatasetSpec::File {
            path: p.clone(),
            sha256: "deadbeef".into(),
        };
        assert!(matches!(
            materialise(
                &bad,
                None,
                dir.path(),
                &ledger,
                &ExecutionControl::default()
            ),
            Err(SimorphError::Validation(_))
        ));
        let good = DatasetSpec::File {
            path: p.clone(),
            sha256: sha256_file(&p).unwrap(),
        };
        let d = materialise(
            &good,
            None,
            dir.path(),
            &ledger,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(d.rows, 1);
    }

    fn adapter_dir() -> String {
        simorph_adapter_python_bridge::bundled_python_dir()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn templates_compile_and_numpy_variant_trains_through_the_bridge() {
        if let Err(r) = simorph_adapter_python_bridge::python_with_numpy_available() {
            eprintln!("SKIPPED: {r}");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for (kind, name) in [
            (TemplateKind::NumpyLogistic, "numpy.py"),
            (TemplateKind::TorchSft, "torch.py"),
            (TemplateKind::TrlPeftLora, "trl.py"),
        ] {
            let p = dir.path().join(name);
            std::fs::write(&p, render(kind, &adapter_dir())).unwrap();
            let out = Command::new(simorph_adapter_python_bridge::python_interpreter())
                .args(["-I", "-m", "py_compile", p.to_string_lossy().as_ref()])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        // dataset: linearly separable 2-D points
        let data = dir.path().join("train.jsonl");
        let mut s = String::new();
        for i in 0..200 {
            let x = (i as f64 * 0.37).sin() * 2.0;
            let y = (i as f64 * 0.91).cos() * 2.0;
            let label = if x + y > 0.0 { 1 } else { 0 };
            s.push_str(&format!(
                "{{\"inputs\":{{\"x\":[{x},{y}]}},\"outputs\":{{\"y\":{label}}}}}\n"
            ));
        }
        std::fs::write(&data, &s).unwrap();
        let ledger = Ledger::in_memory();
        let svc = TrainingService::new(ledger, dir.path().join("work"));
        let job = TrainingJob {
            candidate: Candidate::new(
                "w0",
                [("weights".to_string(), json!([0.0, 0.0, 0.0]))]
                    .into_iter()
                    .collect(),
                "base",
            ),
            trainer: TrainerSpec::Python {
                script: dir.path().join("numpy.py"),
                args: vec![],
            },
            data: DatasetSpec::File {
                path: data.clone(),
                sha256: sha256_file(&data).unwrap(),
            },
            parameter: "weights".into(),
            trainer_inputs: [("learning_rate".to_string(), json!(0.5))]
                .into_iter()
                .collect(),
            budget: TrainingBudget {
                max_wall_time_s: 60.0,
                epochs: 300.0,
            },
            sandbox: SandboxPolicy::strict().with_timeout(60.0),
            seed: 0,
        };
        let r = svc
            .run(&job, &resolver(), &ExecutionControl::default())
            .unwrap();
        let w: Vec<f64> =
            serde_json::from_value(r.new_candidate.parameters["weights"].clone()).unwrap();
        assert!(w[0] > 0.5 && w[1] > 0.5, "weights {w:?}");
        assert!(r.outputs["train_log_loss"].as_f64().unwrap() < 0.3);
        assert_eq!(r.metrics["backend_numpy"], 1.0);
    }

    #[test]
    fn torch_template_reports_unavailability_instead_of_training() {
        if let Err(r) = simorph_adapter_python_bridge::python_with_numpy_available() {
            eprintln!("SKIPPED: {r}");
            return;
        }
        let probe = Command::new(simorph_adapter_python_bridge::python_interpreter())
            .args(["-I", "-c", "import torch"])
            .output()
            .unwrap();
        if probe.status.success() {
            eprintln!("torch is installed here; unavailability path not exercised");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("torch.py");
        std::fs::write(&script, render(TemplateKind::TorchSft, &adapter_dir())).unwrap();
        let data = dir.path().join("d.jsonl");
        std::fs::write(&data, "{\"inputs\":{\"x\":[1]},\"outputs\":{\"y\":1}}\n").unwrap();
        let svc = TrainingService::new(Ledger::in_memory(), dir.path().join("work"));
        let job = TrainingJob {
            candidate: Candidate::new(
                "ckpt",
                [("checkpoint_in".to_string(), json!("none"))]
                    .into_iter()
                    .collect(),
                "base",
            ),
            trainer: TrainerSpec::Python {
                script,
                args: vec![],
            },
            data: DatasetSpec::File {
                path: data.clone(),
                sha256: sha256_file(&data).unwrap(),
            },
            parameter: "checkpoint_out".into(),
            trainer_inputs: [
                ("learning_rate".to_string(), json!(1e-3)),
                ("batch_size".to_string(), json!(8)),
            ]
            .into_iter()
            .collect(),
            budget: TrainingBudget {
                max_wall_time_s: 60.0,
                epochs: 1.0,
            },
            sandbox: SandboxPolicy::strict().with_timeout(60.0),
            seed: 0,
        };
        let err = svc
            .run(&job, &resolver(), &ExecutionControl::default())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("torch unavailable"), "{msg}");
    }
}
