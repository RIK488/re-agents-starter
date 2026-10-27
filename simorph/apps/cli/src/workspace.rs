//! Workspace: data directory holding the provenance ledger, experiment
//! archive, campaign reports and lab reports.

use simorph_core::Result;
use simorph_experiment::ExperimentArchive;
use simorph_provenance::{JsonlStore, Ledger};
use std::path::{Path, PathBuf};

pub struct Workspace {
    pub root: PathBuf,
    pub ledger: Ledger,
    pub archive: ExperimentArchive,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("campaigns"))?;
        std::fs::create_dir_all(root.join("lab"))?;
        std::fs::create_dir_all(root.join("runs"))?;
        let ledger = Ledger::new(Box::new(JsonlStore::open(root.join("provenance.jsonl"))?));
        let archive = ExperimentArchive::open(root.join("experiments"))?;
        Ok(Self {
            root,
            ledger,
            archive,
        })
    }
    pub fn ledger_path(&self) -> PathBuf {
        self.root.join("provenance.jsonl")
    }
}
