//! Protected registry of sealed suites.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{Permission, Result, SimorphError};
use simorph_security::{AuditLog, HumanApproval, Policy, Principal, PrincipalKind, ProtectedGuard};
use simorph_svc_ai_laboratory::SealedSuite;
use std::path::{Path, PathBuf};

/// Suites keyed by name, kept under a protected guard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteRegistry {
    guard: ProtectedGuard<IndexMap<String, SealedSuite>>,
}

impl Default for SuiteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SuiteRegistry {
    pub fn new() -> Self {
        Self {
            guard: ProtectedGuard::seal(IndexMap::new()),
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.guard.get().keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&SealedSuite> {
        self.guard.get().get(name)
    }

    pub fn seal_hash(&self) -> &str {
        self.guard.seal_hash()
    }

    /// Verify the outer seal and every inner suite seal.
    pub fn verify(&self) -> Result<()> {
        self.guard.verify()?;
        for (name, s) in self.guard.get() {
            s.verify()
                .map_err(|e| SimorphError::Protected(format!("suite `{name}`: {e}")))?;
        }
        Ok(())
    }

    /// Register (or replace) a suite.  Requires a human administrator with an
    /// approval carrying `SelfModify`.  Autonomous agents are refused before
    /// any mutation is attempted, and the refusal is audited.
    pub fn register(
        &mut self,
        policy: &Policy,
        principal: &Principal,
        approval: Option<&HumanApproval>,
        suite: SealedSuite,
    ) -> Result<()> {
        suite.verify()?;
        let requested = simorph_security::perm_set([Permission::SelfModify]);
        if principal.kind == PrincipalKind::AutonomousAgent {
            policy.audit().record(
                principal,
                &requested,
                false,
                "autonomous agent cannot register evaluation suites",
            );
            return Err(SimorphError::Protected(format!(
                "autonomous agent `{}` cannot register or replace evaluation suites",
                principal.id
            )));
        }
        let approval = match approval {
            Some(a) => a,
            None => {
                policy.audit().record(
                    principal,
                    &requested,
                    false,
                    "suite registration without human approval",
                );
                return Err(SimorphError::PermissionDenied(
                    "suite registration requires a human approval".into(),
                ));
            }
        };
        let name = suite.suite().name.clone();
        self.guard.mutate(policy, principal, approval, |m| {
            m.insert(name, suite);
        })
    }

    pub fn save(&self, dir: impl AsRef<Path>) -> Result<PathBuf> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = dir.join("suites.json");
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(path)
    }

    /// Load and verify every seal; a tampered file is refused.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let path = dir.as_ref().join("suites.json");
        let bytes = std::fs::read(&path)?;
        let reg: Self = serde_json::from_slice(&bytes)?;
        reg.verify()?;
        Ok(reg)
    }

    pub fn audit_summary(audit: &AuditLog) -> Vec<String> {
        audit.entries().iter().map(|e| format!("{:?}", e)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard::standard_suites;
    use chrono::Duration;

    fn policy() -> Policy {
        Policy::new(b"secret".to_vec(), AuditLog::in_memory())
    }

    #[test]
    fn agent_cannot_register_but_admin_with_approval_can() {
        let policy = policy();
        let mut reg = SuiteRegistry::new();
        let suite = standard_suites().remove(0);
        let agent = Principal::agent("ai-lab");
        let admin = Principal::admin("alice");
        let approval = HumanApproval::issue(
            &admin,
            &admin,
            [Permission::SelfModify],
            Duration::minutes(5),
            policy.signing_secret(),
        )
        .unwrap();
        let agent_approval = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::SelfModify],
            Duration::minutes(5),
            policy.signing_secret(),
        )
        .unwrap();
        assert!(matches!(
            reg.register(&policy, &agent, Some(&agent_approval), suite.clone()),
            Err(SimorphError::Protected(_))
        ));
        assert!(matches!(
            reg.register(&policy, &admin, None, suite.clone()),
            Err(SimorphError::PermissionDenied(_))
        ));
        reg.register(&policy, &admin, Some(&approval), suite.clone())
            .unwrap();
        assert_eq!(reg.names(), vec![suite.suite().name.clone()]);
        reg.verify().unwrap();
        let denied = policy
            .audit()
            .entries()
            .iter()
            .filter(|e| !e.granted)
            .count();
        assert!(denied >= 2, "refusals must be audited");
    }

    #[test]
    fn tampered_persisted_registry_is_rejected() {
        let policy = policy();
        let mut reg = SuiteRegistry::new();
        let admin = Principal::admin("alice");
        let approval = HumanApproval::issue(
            &admin,
            &admin,
            [Permission::SelfModify],
            Duration::minutes(5),
            policy.signing_secret(),
        )
        .unwrap();
        reg.register(
            &policy,
            &admin,
            Some(&approval),
            standard_suites().remove(0),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = reg.save(dir.path()).unwrap();
        SuiteRegistry::load(dir.path()).unwrap().verify().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replacen("\"seeds\": [", "\"seeds\": [\n            777,", 1);
        assert_ne!(text, tampered);
        std::fs::write(&path, tampered).unwrap();
        assert!(matches!(
            SuiteRegistry::load(dir.path()),
            Err(SimorphError::Protected(_))
        ));
    }
}
