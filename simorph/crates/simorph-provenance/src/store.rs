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
    // audit: the default `last()` cloned the whole store, making every append
    // O(n) and a ledger of n records O(n²) to build.
    fn last(&self) -> Option<ProvenanceRecord> {
        self.records.last().cloned()
    }
}

/// Anchor stored next to a JSON-lines ledger (`<ledger>.head`): number of
/// records and hash of the last one, MAC-ed under the key for keyed stores.
/// It detects truncation (dropping the tail keeps a valid chain) and, when
/// keyed, rewriting the whole file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeadAnchor {
    pub count: u64,
    pub last_hash: Option<String>,
    #[serde(default)]
    pub mac: Option<String>,
}

impl HeadAnchor {
    fn message(&self) -> String {
        format!("{}:{}", self.count, self.last_hash.as_deref().unwrap_or(""))
    }
    fn sign(mut self, key: Option<&[u8]>) -> Self {
        self.mac = key.map(|k| simorph_core::hashing::hmac_sha256_hex(k, self.message().as_bytes()));
        self
    }
    fn verify(&self, key: Option<&[u8]>) -> bool {
        match key {
            None => true,
            Some(k) => self
                .mac
                .as_deref()
                .map(|m| {
                    simorph_core::hashing::ct_eq(
                        m,
                        &simorph_core::hashing::hmac_sha256_hex(k, self.message().as_bytes()),
                    )
                })
                .unwrap_or(false),
        }
    }
}

/// JSON-lines file store.  Loading verifies every record's own hash, its
/// sequence number and the chain linkage; a corrupted file is refused.
/// Appends go through one open handle and are flushed with `sync_data`
/// (disable with [`JsonlStore::with_sync`] for bulk imports: the head
/// anchor is then refreshed every 256 appends and when the store is dropped;
/// a stale anchor still detects truncation below it).
///
/// **Integrity model.**  Unkeyed ([`JsonlStore::open`]): detects accidental
/// corruption and truncation below the `.head` anchor, but anyone who can
/// write the files can edit a record and recompute every hash.  Keyed
/// ([`JsonlStore::open_keyed`]): records and the anchor carry HMAC-SHA256
/// tags; edit-and-rehash, record removal and truncation are detected by
/// whoever holds the key (which must be stored elsewhere).
#[derive(Debug)]
pub struct JsonlStore {
    path: PathBuf,
    memory: MemoryStore,
    file: Option<File>,
    key: Option<Vec<u8>>,
    sync: bool,
}

impl JsonlStore {
    /// Open (or create) an unkeyed ledger file and verify it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path.as_ref(), None)
    }

    /// Open (or create) a keyed ledger file: every record must carry a valid
    /// MAC and the `.head` anchor must be present (for a non-empty file) and
    /// authentic.
    pub fn open_keyed(path: impl AsRef<Path>, key: &[u8]) -> Result<Self> {
        if key.is_empty() {
            return Err(SimorphError::InvalidInput("empty ledger key".into()));
        }
        Self::open_inner(path.as_ref(), Some(key))
    }

    /// Path of the head anchor of a ledger file.
    pub fn head_path(path: &Path) -> PathBuf {
        let mut p = path.as_os_str().to_owned();
        p.push(".head");
        PathBuf::from(p)
    }

    fn open_inner(path: &Path, key: Option<&[u8]>) -> Result<Self> {
        let path = path.to_path_buf();
        let mut memory = MemoryStore::new();
        let mut hashes: Vec<String> = vec![];
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
                if let Some(k) = key {
                    if !rec.is_authentic(k) {
                        return Err(SimorphError::Validation(format!(
                            "provenance record {} (line {}) has no valid MAC for the ledger key \
                             (edited and re-hashed, or written without the key)",
                            rec.id,
                            lineno + 1
                        )));
                    }
                }
                if rec.previous_hash != prev_hash {
                    return Err(SimorphError::Validation(format!(
                        "provenance chain broken at record {} (line {})",
                        rec.id,
                        lineno + 1
                    )));
                }
                // audit: sequence numbers were not checked on load
                if rec.sequence != memory.len() as u64 {
                    return Err(SimorphError::Validation(format!(
                        "provenance record {} (line {}) has sequence {} where {} was expected",
                        rec.id,
                        lineno + 1,
                        rec.sequence,
                        memory.len()
                    )));
                }
                prev_hash = Some(rec.content_hash.clone());
                hashes.push(rec.content_hash.clone());
                memory.append(&rec)?;
            }
        }
        let head_path = Self::head_path(&path);
        if head_path.exists() {
            let head: HeadAnchor = serde_json::from_slice(&std::fs::read(&head_path)?)
                .map_err(|e| SimorphError::Serialization(format!("ledger head: {e}")))?;
            if !head.verify(key) {
                return Err(SimorphError::Validation(
                    "ledger head anchor has no valid MAC for the ledger key".into(),
                ));
            }
            let n = hashes.len() as u64;
            // records appended after the anchor (crash between the two
            // writes) are accepted; fewer records or a different record at
            // the anchored position is truncation / rewriting.
            if n < head.count {
                return Err(SimorphError::Validation(format!(
                    "ledger truncated: the head anchor records {} records, the file holds {n}",
                    head.count
                )));
            }
            let at = if head.count == 0 { None } else { hashes.get(head.count as usize - 1).cloned() };
            if at != head.last_hash {
                return Err(SimorphError::Validation(
                    "ledger rewritten: the record at the anchored position does not match the head anchor"
                        .into(),
                ));
            }
        } else if key.is_some() && !hashes.is_empty() {
            return Err(SimorphError::Validation(
                "keyed ledger has records but no head anchor (deleted?)".into(),
            ));
        }
        // the anchor is (re)written on the next append; opening never
        // modifies the files, so verification can run on read-only copies
        Ok(Self {
            path,
            memory,
            file: None,
            key: key.map(|k| k.to_vec()),
            sync: true,
        })
    }

    /// Enable or disable `sync_data` after each append (default enabled).
    pub fn with_sync(mut self, sync: bool) -> Self {
        self.sync = sync;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_keyed(&self) -> bool {
        self.key.is_some()
    }

    fn write_head(&self) -> Result<()> {
        let head = HeadAnchor {
            count: self.memory.len() as u64,
            last_hash: self.memory.records.last().map(|r| r.content_hash.clone()),
            mac: None,
        }
        .sign(self.key.as_deref());
        let hp = Self::head_path(&self.path);
        let mut tmp = hp.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&serde_json::to_vec(&head)?)?;
            if self.sync {
                f.sync_data()?;
            }
        }
        std::fs::rename(&tmp, &hp)?;
        Ok(())
    }
}

impl Drop for JsonlStore {
    fn drop(&mut self) {
        if self.file.is_some() && !self.memory.is_empty() {
            let _ = self.write_head();
        }
    }
}

impl ProvenanceStore for JsonlStore {
    fn append(&mut self, record: &ProvenanceRecord) -> Result<()> {
        let mut record = record.clone();
        if let Some(k) = &self.key {
            match &record.mac {
                None => record.mac = Some(record.compute_mac(k)),
                Some(_) if !record.is_authentic(k) => {
                    return Err(SimorphError::Validation(format!(
                        "record {} carries a MAC that does not match the store key",
                        record.id
                    )))
                }
                Some(_) => {}
            }
        }
        if self.memory.index.contains_key(&record.id) {
            return Err(SimorphError::InvalidInput(format!(
                "duplicate record id {}",
                record.id
            )));
        }
        if self.file.is_none() {
            self.file = Some(OpenOptions::new().create(true).append(true).open(&self.path)?);
        }
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        let f = self.file.as_mut().expect("opened above");
        f.write_all(&line)?;
        f.flush()?;
        if self.sync {
            f.sync_data()?;
        }
        self.memory.append(&record)?;
        if self.sync || self.memory.len() % 256 == 0 {
            self.write_head()?;
        }
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
    fn last(&self) -> Option<ProvenanceRecord> {
        self.memory.last()
    }
}
