//! JSON archive of experiment records and provenance hook.

use crate::plan::ExperimentRecord;
use simorph_core::{ExperimentId, Result, SimorphError};
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use std::path::{Path, PathBuf};

/// Something that records experiments into a provenance ledger.
pub trait ProvenanceHook {
    /// Record the experiment and its runs; returns the experiment record id.
    fn record_experiment(
        &self,
        record: &ExperimentRecord,
        parents: &[RecordId],
    ) -> Result<RecordId>;
}

impl ProvenanceHook for Ledger {
    fn record_experiment(
        &self,
        record: &ExperimentRecord,
        parents: &[RecordId],
    ) -> Result<RecordId> {
        let env = self.capture_environment()?;
        let mut all_parents = parents.to_vec();
        all_parents.push(env);
        let exp_payload = serde_json::json!({
            "experiment_id": record.id,
            "name": record.plan.name,
            "plan_hash": record.plan_hash,
            "component_hash": record.component_hash,
            "component_id": record.plan.component_id,
            "n_runs": record.runs.len(),
            "n_success": record.n_success(),
            "outputs_hash": record.outputs_hash,
            "criteria_met": record.all_criteria_met(),
            "started_at": record.started_at,
            "finished_at": record.finished_at,
        });
        let exp = self.append(
            ProvenanceKind::Experiment,
            record.plan.name.clone(),
            exp_payload,
            &all_parents,
        )?;
        for r in &record.runs {
            let payload = serde_json::json!({
                "run_id": r.run_id,
                "index": r.index,
                "seed": r.seed,
                "inputs": r.inputs,
                "status": r.status,
                "request_hash": r.result.as_ref().map(|x| x.request_hash.clone()),
                "output_hash": r.result.as_ref().map(|x| x.output_hash()),
                "evidence": r.result.as_ref().map(|x| x.evidence),
                "wall_time_ms": r.wall_time_ms,
            });
            let kind = if r.is_success() {
                ProvenanceKind::Run
            } else {
                ProvenanceKind::Failure
            };
            self.append(
                kind,
                format!("{} #{}", record.plan.name, r.index),
                payload,
                &[exp],
            )?;
        }
        Ok(exp)
    }
}

/// Directory of `<experiment-id>.json` files.
#[derive(Debug, Clone)]
pub struct ExperimentArchive {
    dir: PathBuf,
}

impl ExperimentArchive {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, id: ExperimentId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Store a record.  Refuses to overwrite an existing record with a
    /// different content (archives are append-only per experiment id).
    pub fn store(&self, record: &ExperimentRecord) -> Result<PathBuf> {
        let p = self.path(record.id);
        let json = serde_json::to_string_pretty(record)?;
        if p.exists() {
            let existing = std::fs::read_to_string(&p)?;
            if existing != json {
                return Err(SimorphError::Protected(format!(
                    "experiment {} already archived with different content",
                    record.id
                )));
            }
            return Ok(p);
        }
        std::fs::write(&p, json)?;
        Ok(p)
    }

    pub fn get(&self, id: ExperimentId) -> Result<ExperimentRecord> {
        let p = self.path(id);
        if !p.exists() {
            return Err(SimorphError::NotFound(format!("experiment {id}")));
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?)
    }

    pub fn list(&self) -> Result<Vec<ExperimentId>> {
        let mut out = vec![];
        for e in std::fs::read_dir(&self.dir)? {
            let e = e?;
            if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
                if e.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(id) = ExperimentId::parse(stem) {
                        out.push(id);
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }
}
