//! Datasets with provenance.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_experiment::{run_experiment, Design, ExperimentPlan, ProvenanceHook};
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatasetSpec {
    /// Run `component` over `design` and store (inputs, outputs) rows.
    Synthetic {
        component: ComponentId,
        design: Design,
        #[serde(default)]
        fixed_inputs: IndexMap<String, serde_json::Value>,
        seed: u64,
    },
    /// Existing file whose SHA-256 must match.
    File { path: PathBuf, sha256: String },
}

/// A materialised dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dataset {
    pub id: DatasetId,
    pub path: PathBuf,
    pub sha256: String,
    pub rows: usize,
    /// Provenance record of the dataset (synthetic: the experiment that produced it).
    pub provenance: Option<RecordId>,
    pub description: String,
}

pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(simorph_core::hashing::sha256_hex(&std::fs::read(path)?))
}

/// Materialise a dataset spec.  Synthetic datasets are written to `out_dir`
/// as JSON-lines (`{"inputs": {...}, "outputs": {...}, "run_id": ...}`).
pub fn materialise(
    spec: &DatasetSpec,
    component: Option<&dyn Component>,
    out_dir: &Path,
    ledger: &Ledger,
    ctl: &ExecutionControl,
) -> Result<Dataset> {
    match spec {
        DatasetSpec::File { path, sha256 } => {
            let actual = sha256_file(path)?;
            if &actual != sha256 {
                return Err(SimorphError::Validation(format!(
                    "dataset {} hash mismatch: expected {sha256}, found {actual}",
                    path.display()
                )));
            }
            let rows = std::fs::read_to_string(path)?
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count();
            let id = DatasetId::deterministic(sha256);
            let prov = ledger
                .record(
                    ProvenanceKind::Dataset,
                    format!("jeu de données fichier {}", path.display()),
                    &serde_json::json!({"path": path, "sha256": sha256, "rows": rows}),
                    &[],
                )
                .ok();
            Ok(Dataset {
                id,
                path: path.clone(),
                sha256: actual,
                rows,
                provenance: prov,
                description: "fichier fourni, empreinte vérifiée".into(),
            })
        }
        DatasetSpec::Synthetic {
            component: cid,
            design,
            fixed_inputs,
            seed,
        } => {
            let comp = component.ok_or_else(|| {
                SimorphError::NotFound(format!("component {cid} for synthetic dataset"))
            })?;
            if comp.manifest().id != *cid {
                return Err(SimorphError::InvalidInput(
                    "component does not match the dataset spec".into(),
                ));
            }
            let mut plan =
                ExperimentPlan::new("génération de données", *cid, design.clone(), *seed);
            plan.fixed_inputs = fixed_inputs.clone();
            let record = run_experiment(&plan, comp, ctl)?;
            std::fs::create_dir_all(out_dir)?;
            let path = out_dir.join(format!("dataset-{}.jsonl", record.id));
            let mut f = std::fs::File::create(&path)?;
            let mut rows = 0usize;
            for r in &record.runs {
                if let Some(res) = &r.result {
                    let line = serde_json::json!({"run_id": r.run_id, "inputs": r.inputs, "outputs": res.outputs, "evidence": res.evidence});
                    writeln!(f, "{}", serde_json::to_string(&line)?)?;
                    rows += 1;
                }
            }
            drop(f);
            let sha = sha256_file(&path)?;
            let exp = ledger.record_experiment(&record, &[])?;
            let prov = ledger.record(ProvenanceKind::Dataset, format!("jeu de données synthétique ({rows} lignes)"), &serde_json::json!({"path": path, "sha256": sha, "rows": rows, "failed_runs": record.n_failed()}), &[exp])?;
            Ok(Dataset {
                id: DatasetId::deterministic(&sha),
                path,
                sha256: sha,
                rows,
                provenance: Some(prov),
                description: format!(
                    "généré par {} sur un plan {:?} de {} points",
                    comp.manifest().name,
                    design.kind,
                    design.len()
                ),
            })
        }
    }
}
