//! Storage back-ends.

use crate::record::{ProvenanceRecord, RecordId};
use simorph_core::{Result, SimorphError};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Append-only record storage.  Implementations must never allow updates or
/// deletions; `append` is the only mutating operation.
pub trait ProvenanceStore: Send + Sync {
    fn append(&mut self, record: &ProvenanceRecord) -> Result<()>;
    fn get(&self, id: RecordId) -> Option<ProvenanceRecord>;
    /// All records in chain order.
    fn all(&self) -> Vec<ProvenanceRecord>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn last(&self) -> Option<ProvenanceRecord> {
        self.all().last().cloned()
    }
}

/// In-memory store.
#[derive(Default, Debug)]
pub struct MemoryStore {
    records: Vec<ProvenanceRecord>,
    index: HashMap<RecordId, usize>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProvenanceStore for MemoryStore {
    fn append(&mut self, record: &ProvenanceRecord) -> Result<()> {
        if self.index.contains_key(&record.id) {
            return Err(SimorphError::InvalidInput(format!(
                "duplicate record id {}",
                record.id
            )));
        }
        self.index.insert(record.id, self.records.len());
        self.records.push(record.clone());
        Ok(())
    }
    fn get(&self, id: RecordId) -> Option<ProvenanceRecord> {
        self.index.get(&id).map(|&i| self.records[i].clone())
    }
    fn all(&self) -> Vec<ProvenanceRecord> {
        self.records.clone()
    }
    fn len(&self) -> usize {
        self.records.len()
    }
}

/// JSON-lines file store.  Every append is flushed to disk.  Loading verifies
/// every record's own hash and the chain linkage; a corrupted file is refused.
#[derive(Debug)]
pub struct JsonlStore {
    path: PathBuf,
    memory: MemoryStore,
}

impl JsonlStore {
    /// Open (or create) a ledger file and verify it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut memory = MemoryStore::new();
        if path.exists() {
            let f = File::open(&path)?;
            let reader = BufReader::new(f);
            let mut prev_hash: Option<String> = None;
            for (lineno, line) in reader.lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let rec: ProvenanceRecord = serde_json::from_str(&line).map_err(|e| {
                    SimorphError::Serialization(format!("line {}: {e}", lineno + 1))
                })?;
                if !rec.is_intact() {
                    return Err(SimorphError::Validation(format!(
                        "provenance record {} (line {}) has been tampered with: hash mismatch",
                        rec.id,
                        lineno + 1
                    )));
                }
                if rec.previous_hash != prev_hash {
                    return Err(SimorphError::Validation(format!(
                        "provenance chain broken at record {} (line {})",
                        rec.id,
                        lineno + 1
                    )));
                }
                prev_hash = Some(rec.content_hash.clone());
                memory.append(&rec)?;
            }
        }
        Ok(Self { path, memory })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ProvenanceStore for JsonlStore {
    fn append(&mut self, record: &ProvenanceRecord) -> Result<()> {
        self.memory.append(record)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(record)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()?;
        Ok(())
    }
    fn get(&self, id: RecordId) -> Option<ProvenanceRecord> {
        self.memory.get(id)
    }
    fn all(&self) -> Vec<ProvenanceRecord> {
        self.memory.all()
    }
    fn len(&self) -> usize {
        self.memory.len()
    }
}
