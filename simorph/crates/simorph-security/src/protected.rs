//! Sealed resources whose modification requires an administrator and a human
//! approval.  Used for validation references, evaluation baselines and
//! safety configuration: the scientific director and the AI laboratory can
//! read them but can never alter the criteria by which they are judged.

use crate::perm::{perm_set, Perm};
use crate::rbac::{HumanApproval, Policy, Principal};
use serde::{Deserialize, Serialize};
use simorph_core::{hashing, Permission, Result, SimorphError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedGuard<T> {
    value: T,
    seal: String,
    /// History of (approver, previous seal) for every accepted mutation.
    history: Vec<(String, String)>,
}

impl<T: Serialize> ProtectedGuard<T> {
    /// Seal a value.  Anyone can create a guard; only admins can change it.
    pub fn seal(value: T) -> Self {
        let seal = hashing::hash_json(&value).expect("serialisable");
        Self {
            value,
            seal,
            history: vec![],
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn seal_hash(&self) -> &str {
        &self.seal
    }

    pub fn history(&self) -> &[(String, String)] {
        &self.history
    }

    /// Recompute the hash and compare with the seal; detects any out-of-band
    /// modification (e.g. edited on disk).
    pub fn verify(&self) -> Result<()> {
        let now = hashing::hash_json(&self.value)?;
        if now != self.seal {
            return Err(SimorphError::Protected(format!(
                "seal mismatch: expected {}, found {now}",
                self.seal
            )));
        }
        Ok(())
    }

    /// Apply a mutation.  Requires a human administrator *and* a valid human
    /// approval carrying `SelfModify` for that administrator (two-person-rule
    /// friendly: approver and mutator may differ).
    pub fn mutate(
        &mut self,
        policy: &Policy,
        principal: &Principal,
        approval: &HumanApproval,
        f: impl FnOnce(&mut T),
    ) -> Result<()> {
        if !principal.is_admin() {
            policy.audit().record(
                principal,
                &perm_set([Permission::SelfModify]),
                false,
                "protected mutation by non-admin",
            );
            return Err(SimorphError::Protected(format!(
                "`{}` is not an administrator",
                principal.id
            )));
        }
        policy.authorize(principal, [Permission::SelfModify], Some(approval))?;
        if !approval.scope.contains(&Perm(Permission::SelfModify)) {
            return Err(SimorphError::Protected(
                "approval scope lacks SelfModify".into(),
            ));
        }
        self.verify()?;
        let previous = self.seal.clone();
        f(&mut self.value);
        self.seal = hashing::hash_json(&self.value)?;
        self.history.push((approval.approver.clone(), previous));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditLog;
    use chrono::Duration;

    #[test]
    fn agents_and_non_admins_cannot_mutate() {
        let policy = Policy::new(b"k".to_vec(), AuditLog::in_memory());
        let admin = Principal::admin("alice");
        let agent = Principal::agent("director");
        let mut g = ProtectedGuard::seal(vec![1.0, 2.0]);
        let approval_for_agent = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::SelfModify],
            Duration::minutes(1),
            policy.signing_secret(),
        )
        .unwrap();
        let r = g.mutate(&policy, &agent, &approval_for_agent, |v| v.push(3.0));
        assert!(matches!(r, Err(SimorphError::Protected(_))));
        assert_eq!(g.get().len(), 2);
    }

    #[test]
    fn admin_with_approval_can_mutate_and_history_is_kept() {
        let policy = Policy::new(b"k".to_vec(), AuditLog::in_memory());
        let admin = Principal::admin("alice");
        let mut g = ProtectedGuard::seal(vec![1.0, 2.0]);
        let before = g.seal_hash().to_string();
        let approval = HumanApproval::issue(
            &admin,
            &admin,
            [Permission::SelfModify],
            Duration::minutes(1),
            policy.signing_secret(),
        )
        .unwrap();
        g.mutate(&policy, &admin, &approval, |v| v.push(3.0))
            .unwrap();
        assert_eq!(g.get().len(), 3);
        assert_ne!(g.seal_hash(), before);
        assert_eq!(g.history(), &[("alice".to_string(), before)]);
        g.verify().unwrap();
    }

    #[test]
    fn tampering_is_detected() {
        let g = ProtectedGuard::seal(vec![1, 2, 3]);
        let json = serde_json::to_string(&g).unwrap();
        let tampered = json.replace("[1,2,3]", "[1,2,4]");
        let g2: ProtectedGuard<Vec<i32>> = serde_json::from_str(&tampered).unwrap();
        assert!(matches!(g2.verify(), Err(SimorphError::Protected(_))));
    }
}
