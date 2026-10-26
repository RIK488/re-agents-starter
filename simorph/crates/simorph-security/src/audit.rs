//! Append-only audit log of authorisation decisions.

use crate::perm::PermSet;
use crate::rbac::Principal;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub principal: String,
    pub requested: PermSet,
    pub granted: bool,
    pub reason: String,
}

/// Thread-safe audit log.  Entries are kept in memory and, when a path is
/// configured, appended as JSON lines.  There is no API to remove entries.
#[derive(Debug, Clone)]
pub struct AuditLog {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    entries: Vec<AuditEntry>,
    path: Option<PathBuf>,
}

impl AuditLog {
    pub fn in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: vec![],
                path: None,
            })),
        }
    }

    pub fn with_file(path: impl AsRef<Path>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: vec![],
                path: Some(path.as_ref().to_path_buf()),
            })),
        }
    }

    pub fn record(&self, principal: &Principal, requested: &PermSet, granted: bool, reason: &str) {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            principal: principal.id.clone(),
            requested: requested.clone(),
            granted,
            reason: reason.to_string(),
        };
        let mut g = self.inner.lock();
        if let Some(p) = &g.path {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
            {
                if let Ok(line) = serde_json::to_string(&entry) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
        g.entries.push(entry);
    }

    pub fn entries(&self) -> Vec<AuditEntry> {
        self.inner.lock().entries.clone()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load entries from a JSON-lines file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Vec<AuditEntry>> {
        let text = std::fs::read_to_string(path)?;
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perm::perm_set;
    use simorph_core::Permission;

    #[test]
    fn file_log_persists_entries() {
        let dir = std::env::temp_dir().join(format!("simorph-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audit.jsonl");
        let log = AuditLog::with_file(&path);
        let p = Principal::agent("a");
        log.record(&p, &perm_set([Permission::ReadWorkspace]), true, "granted");
        log.record(&p, &perm_set([Permission::SelfModify]), false, "denied");
        let loaded = AuditLog::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(!loaded[1].granted);
        std::fs::remove_dir_all(dir).ok();
    }
}
