//! Secrets: allow-listed access and log redaction.

use parking_lot::RwLock;
use simorph_core::{Result, SimorphError};
use std::collections::{BTreeMap, BTreeSet};

/// Secret store backed by the process environment (or an explicit map),
/// exposing only allow-listed names.  Values are never logged by the store.
#[derive(Debug)]
pub struct SecretStore {
    allow: BTreeSet<String>,
    values: RwLock<BTreeMap<String, String>>,
}

impl SecretStore {
    /// Read allow-listed names from the environment.
    pub fn from_env(allow: &[&str]) -> Self {
        let mut values = BTreeMap::new();
        for name in allow {
            if let Ok(v) = std::env::var(name) {
                values.insert(name.to_string(), v);
            }
        }
        Self {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            values: RwLock::new(values),
        }
    }

    /// Explicit map (tests, injected configuration).
    pub fn from_map(allow: &[&str], map: BTreeMap<String, String>) -> Self {
        let allow: BTreeSet<String> = allow.iter().map(|s| s.to_string()).collect();
        let values = map.into_iter().filter(|(k, _)| allow.contains(k)).collect();
        Self {
            allow,
            values: RwLock::new(values),
        }
    }

    pub fn get(&self, name: &str) -> Result<String> {
        if !self.allow.contains(name) {
            return Err(SimorphError::PermissionDenied(format!(
                "secret `{name}` is not allow-listed"
            )));
        }
        self.values
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| SimorphError::NotFound(format!("secret `{name}` not set")))
    }

    pub fn names(&self) -> Vec<String> {
        self.allow.iter().cloned().collect()
    }

    /// A redactor masking every known secret value.
    pub fn redactor(&self) -> Redactor {
        Redactor::new(self.values.read().values().cloned())
    }
}

/// Masks secret values in arbitrary text (log lines, error messages,
/// subprocess output).
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets.into_iter().filter(|s| s.len() >= 4).collect();
        // longest first so that overlapping secrets are fully masked
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        Self { secrets }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.secrets {
            if out.contains(s.as_str()) {
                out = out.replace(s.as_str(), "[REDACTED]");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_and_redaction() {
        let map: BTreeMap<String, String> = [
            ("API_KEY".to_string(), "sk-abcdef123456".to_string()),
            ("OTHER".to_string(), "visible".to_string()),
        ]
        .into_iter()
        .collect();
        let store = SecretStore::from_map(&["API_KEY"], map);
        assert_eq!(store.get("API_KEY").unwrap(), "sk-abcdef123456");
        assert!(matches!(
            store.get("OTHER"),
            Err(SimorphError::PermissionDenied(_))
        ));
        let r = store.redactor();
        assert_eq!(
            r.redact("token sk-abcdef123456 used"),
            "token [REDACTED] used"
        );
        assert_eq!(r.redact("nothing"), "nothing");
    }
}
