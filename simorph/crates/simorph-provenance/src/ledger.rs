//! Thread-safe ledger with queries.

use crate::environment::EnvironmentCapture;
use crate::record::{ProvenanceKind, ProvenanceRecord, RecordId};
use crate::store::{MemoryStore, ProvenanceStore};
use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// One step of a causal explanation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplanationStep {
    pub depth: usize,
    pub record: ProvenanceRecord,
}

/// Why a record exists: its ancestors ordered from root causes to the
/// target (problem → hypotheses → experiments → runs → validations →
/// decision).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub target: RecordId,
    pub steps: Vec<ExplanationStep>,
}

impl Explanation {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Trace de provenance pour {}\n\n", self.target));
        for s in &self.steps {
            let indent = "  ".repeat(s.depth);
            out.push_str(&format!(
                "{indent}- [{}] **{}** `{}` ({})\n",
                s.record.kind.label_fr(),
                s.record.label,
                s.record.id,
                s.record.timestamp.to_rfc3339()
            ));
        }
        out
    }
    pub fn kinds(&self) -> Vec<ProvenanceKind> {
        self.steps.iter().map(|s| s.record.kind).collect()
    }
}

/// Append-only ledger.  Cloning shares the underlying store.
#[derive(Clone)]
pub struct Ledger {
    inner: Arc<Mutex<Box<dyn ProvenanceStore>>>,
    environment_record: Arc<Mutex<Option<RecordId>>>,
    key: Option<Arc<Vec<u8>>>,
}

impl Ledger {
    /// Unkeyed ledger: the hash chain detects accidental corruption only —
    /// anyone with write access can edit a record and re-hash the chain.
    pub fn new(store: Box<dyn ProvenanceStore>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(store)),
            environment_record: Arc::new(Mutex::new(None)),
            key: None,
        }
    }

    /// Keyed ledger: every record carries an HMAC-SHA256 under `key`, and
    /// [`Self::verify_chain`] requires it, so edit-and-rehash is detected by
    /// whoever holds the key.  Keep the key outside the ledger's storage.
    pub fn with_key(store: Box<dyn ProvenanceStore>, key: &[u8]) -> Self {
        let mut l = Self::new(store);
        l.key = Some(Arc::new(key.to_vec()));
        l
    }

    pub fn is_keyed(&self) -> bool {
        self.key.is_some()
    }

    pub fn in_memory() -> Self {
        Self::new(Box::new(MemoryStore::new()))
    }

    /// Record the execution environment once per session; subsequent calls
    /// return the same record id.
    pub fn capture_environment(&self) -> Result<RecordId> {
        self.capture_environment_with(EnvironmentCapture::capture())
    }

    /// Same as [`Self::capture_environment`] with a caller-built capture
    /// (e.g. [`EnvironmentCapture::capture_for`]).
    pub fn capture_environment_with(&self, env: EnvironmentCapture) -> Result<RecordId> {
        let mut slot = self.environment_record.lock();
        if let Some(id) = *slot {
            return Ok(id);
        }
        let id = self.append(
            ProvenanceKind::Environment,
            "execution environment",
            serde_json::to_value(&env)?,
            &[],
        )?;
        *slot = Some(id);
        Ok(id)
    }

    /// Append a record and return its id.  Parents must already exist.
    pub fn append(
        &self,
        kind: ProvenanceKind,
        label: impl Into<String>,
        payload: serde_json::Value,
        parents: &[RecordId],
    ) -> Result<RecordId> {
        let mut store = self.inner.lock();
        for p in parents {
            if store.get(*p).is_none() {
                return Err(SimorphError::NotFound(format!("parent record {p}")));
            }
        }
        let last = store.last();
        let mut rec = ProvenanceRecord {
            id: RecordId::new(),
            sequence: last.as_ref().map(|r| r.sequence + 1).unwrap_or(0),
            timestamp: Utc::now(),
            kind,
            label: label.into(),
            payload,
            parents: parents.to_vec(),
            previous_hash: last.map(|r| r.content_hash),
            content_hash: String::new(),
            mac: None,
        };
        rec.content_hash = rec.compute_hash();
        if let Some(k) = &self.key {
            rec.mac = Some(rec.compute_mac(k));
        }
        store.append(&rec)?;
        Ok(rec.id)
    }

    /// Convenience: serialise any value as payload.
    pub fn record<T: Serialize>(
        &self,
        kind: ProvenanceKind,
        label: impl Into<String>,
        value: &T,
        parents: &[RecordId],
    ) -> Result<RecordId> {
        self.append(kind, label, serde_json::to_value(value)?, parents)
    }

    pub fn get(&self, id: RecordId) -> Option<ProvenanceRecord> {
        self.inner.lock().get(id)
    }

    pub fn all(&self) -> Vec<ProvenanceRecord> {
        self.inner.lock().all()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn by_kind(&self, kind: ProvenanceKind) -> Vec<ProvenanceRecord> {
        self.all().into_iter().filter(|r| r.kind == kind).collect()
    }

    /// Verify every record hash and the chain linkage.
    pub fn verify_chain(&self) -> Result<()> {
        let all = self.all();
        let mut prev: Option<String> = None;
        for (i, r) in all.iter().enumerate() {
            if !r.is_intact() {
                return Err(SimorphError::Validation(format!(
                    "record {} (#{i}) hash mismatch",
                    r.id
                )));
            }
            if r.previous_hash != prev {
                return Err(SimorphError::Validation(format!(
                    "chain broken at record {} (#{i})",
                    r.id
                )));
            }
            if let Some(k) = &self.key {
                if !r.is_authentic(k) {
                    return Err(SimorphError::Validation(format!(
                        "record {} (#{i}) has no valid MAC for the ledger key",
                        r.id
                    )));
                }
            }
            if r.sequence != i as u64 {
                return Err(SimorphError::Validation(format!(
                    "sequence gap at record {} (#{i})",
                    r.id
                )));
            }
            prev = Some(r.content_hash.clone());
        }
        Ok(())
    }

    /// All transitive parents of `id` (excluding itself), nearest first.
    pub fn ancestors(&self, id: RecordId) -> Vec<ProvenanceRecord> {
        let store = self.inner.lock();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        let mut out = vec![];
        if let Some(r) = store.get(id) {
            queue.extend(r.parents.iter().copied());
        }
        while let Some(p) = queue.pop_front() {
            if !seen.insert(p) {
                continue;
            }
            if let Some(r) = store.get(p) {
                queue.extend(r.parents.iter().copied());
                out.push(r);
            }
        }
        out
    }

    /// All records that transitively derive from `id`, in chain order.
    pub fn descendants(&self, id: RecordId) -> Vec<ProvenanceRecord> {
        let all = self.all();
        let mut set: HashSet<RecordId> = HashSet::new();
        set.insert(id);
        let mut out = vec![];
        for r in all {
            if r.id != id && r.parents.iter().any(|p| set.contains(p)) {
                set.insert(r.id);
                out.push(r);
            }
        }
        out
    }

    /// Ordered causal trace: ancestors sorted by chain position (root causes
    /// first), then the target.  `depth` is the distance to the target along
    /// the shortest parent path.
    pub fn explain(&self, id: RecordId) -> Result<Explanation> {
        let target = self
            .get(id)
            .ok_or_else(|| SimorphError::NotFound(format!("record {id}")))?;
        // BFS distances.
        let store = self.inner.lock();
        let mut dist: std::collections::HashMap<RecordId, usize> = std::collections::HashMap::new();
        let mut queue = VecDeque::new();
        dist.insert(id, 0);
        queue.push_back(id);
        while let Some(cur) = queue.pop_front() {
            let d = dist[&cur];
            if let Some(r) = store.get(cur) {
                for p in r.parents {
                    if !dist.contains_key(&p) {
                        dist.insert(p, d + 1);
                        queue.push_back(p);
                    }
                }
            }
        }
        drop(store);
        let mut steps: Vec<ExplanationStep> = self
            .ancestors(id)
            .into_iter()
            .map(|record| ExplanationStep {
                depth: dist.get(&record.id).copied().unwrap_or(0),
                record,
            })
            .collect();
        steps.sort_by_key(|s| s.record.sequence);
        steps.push(ExplanationStep {
            depth: 0,
            record: target,
        });
        Ok(Explanation { target: id, steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonlStore;
    use serde_json::json;

    fn chain(ledger: &Ledger) -> (RecordId, RecordId) {
        let problem = ledger
            .append(
                ProvenanceKind::Problem,
                "nanosat power budget",
                json!({"title":"p"}),
                &[],
            )
            .unwrap();
        let hyp = ledger
            .append(ProvenanceKind::Hypothesis, "h1", json!({}), &[problem])
            .unwrap();
        let exp = ledger
            .append(ProvenanceKind::Experiment, "e1", json!({}), &[hyp])
            .unwrap();
        let run = ledger
            .append(ProvenanceKind::Run, "r1", json!({"x":1}), &[exp])
            .unwrap();
        let val = ledger
            .append(ProvenanceKind::Validation, "v1", json!({}), &[run])
            .unwrap();
        let dec = ledger
            .append(ProvenanceKind::Decision, "d1", json!({}), &[val, hyp])
            .unwrap();
        (problem, dec)
    }

    #[test]
    fn explain_returns_full_causal_path() {
        let l = Ledger::in_memory();
        let (problem, dec) = chain(&l);
        let ex = l.explain(dec).unwrap();
        assert_eq!(
            ex.kinds(),
            vec![
                ProvenanceKind::Problem,
                ProvenanceKind::Hypothesis,
                ProvenanceKind::Experiment,
                ProvenanceKind::Run,
                ProvenanceKind::Validation,
                ProvenanceKind::Decision
            ]
        );
        assert_eq!(ex.steps[0].record.id, problem);
        assert_eq!(ex.steps[0].depth, 2); // problem -> hypothesis -> decision (shortest parent path)
        let md = ex.to_markdown();
        assert!(md.contains("nanosat power budget"));
        assert_eq!(l.descendants(problem).len(), 5);
        assert!(l.verify_chain().is_ok());
    }

    #[test]
    fn tampering_is_detected() {
        let l = Ledger::in_memory();
        chain(&l);
        let mut all = l.all();
        all[3].payload = json!({"x": 2});
        let mut store = MemoryStore::new();
        for r in &all {
            store.append(r).unwrap();
        }
        let tampered = Ledger::new(Box::new(store));
        assert!(tampered.verify_chain().is_err());
    }

    #[test]
    fn jsonl_round_trip_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let l = Ledger::new(Box::new(JsonlStore::open(&path).unwrap()));
        let (_, dec) = chain(&l);
        l.capture_environment().unwrap();
        drop(l);
        let reopened = Ledger::new(Box::new(JsonlStore::open(&path).unwrap()));
        assert_eq!(reopened.len(), 7);
        assert!(reopened.verify_chain().is_ok());
        assert!(reopened.get(dec).is_some());
        // corrupt the file
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"x\":1", "\"x\":9");
        std::fs::write(&path, text).unwrap();
        assert!(JsonlStore::open(&path).is_err());
    }

    #[test]
    fn concurrent_appends_keep_chain_order() {
        let l = Ledger::in_memory();
        let root = l
            .append(ProvenanceKind::Problem, "root", json!({}), &[])
            .unwrap();
        let handles: Vec<_> = (0..8)
            .map(|t| {
                let l = l.clone();
                std::thread::spawn(move || {
                    for i in 0..50 {
                        l.append(ProvenanceKind::Run, format!("t{t}-{i}"), json!({}), &[root])
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(l.len(), 401);
        assert!(l.verify_chain().is_ok());
    }

    #[test]
    fn missing_parent_is_rejected() {
        let l = Ledger::in_memory();
        assert!(l
            .append(ProvenanceKind::Run, "orphan", json!({}), &[RecordId::new()])
            .is_err());
    }

    #[test]
    fn environment_captured_once() {
        let l = Ledger::in_memory();
        let a = l.capture_environment().unwrap();
        let b = l.capture_environment().unwrap();
        assert_eq!(a, b);
        assert_eq!(l.by_kind(ProvenanceKind::Environment).len(), 1);
    }
}
