//! Role-based access control and human approval tokens.

use crate::audit::AuditLog;
use crate::perm::{perm_set, Perm, PermSet};
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{Permission, Result, SimorphError};
use std::collections::BTreeSet;

/// Nature of the entity requesting an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// A human operating the system.
    User,
    /// A trusted internal service (scheduler, registry…).
    Service,
    /// An autonomous agent (scientific director, AI laboratory, generated code).
    AutonomousAgent,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Researcher,
    Operator,
    Viewer,
    Agent,
}

impl Role {
    /// Permissions a role may hold.  Note that `ExternalSideEffect` and
    /// `SelfModify` only appear under `Admin`; everything else must go
    /// through a human approval.
    pub fn permissions(self) -> PermSet {
        use Permission::*;
        let v: Vec<Permission> = match self {
            Role::Admin => vec![
                ReadWorkspace,
                WriteWorkspace,
                NetworkEgress,
                SpawnProcess,
                Gpu,
                ReadSecrets,
                ExternalSideEffect,
                SelfModify,
            ],
            Role::Researcher => vec![
                ReadWorkspace,
                WriteWorkspace,
                NetworkEgress,
                SpawnProcess,
                Gpu,
            ],
            Role::Operator => vec![
                ReadWorkspace,
                WriteWorkspace,
                SpawnProcess,
                Gpu,
                ReadSecrets,
            ],
            Role::Viewer => vec![ReadWorkspace],
            Role::Agent => vec![ReadWorkspace, WriteWorkspace, SpawnProcess, Gpu],
        };
        perm_set(v)
    }
}

/// Identity of a requester.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Principal {
    pub id: String,
    pub kind: PrincipalKind,
    pub roles: BTreeSet<Role>,
}

impl Principal {
    /// Build a principal.  `Admin` cannot be assigned through this
    /// constructor: it is refused for every kind so that no code path can
    /// silently mint an administrator (use [`Principal::admin`] which requires
    /// an explicit, auditable call and is refused for autonomous agents).
    pub fn new(
        id: impl Into<String>,
        kind: PrincipalKind,
        roles: impl IntoIterator<Item = Role>,
    ) -> Result<Self> {
        let roles: BTreeSet<Role> = roles.into_iter().collect();
        if roles.contains(&Role::Admin) {
            return Err(SimorphError::PermissionDenied(
                "Admin role cannot be assigned through Principal::new".into(),
            ));
        }
        Ok(Self {
            id: id.into(),
            kind,
            roles,
        })
    }

    /// Explicit administrator principal.  Only humans may be administrators.
    pub fn admin(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::User,
            roles: [Role::Admin].into_iter().collect(),
        }
    }

    /// Convenience constructor for autonomous agents (role `Agent`).
    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::AutonomousAgent,
            roles: [Role::Agent].into_iter().collect(),
        }
    }

    pub fn is_admin(&self) -> bool {
        self.kind == PrincipalKind::User && self.roles.contains(&Role::Admin)
    }

    pub fn role_permissions(&self) -> PermSet {
        self.roles.iter().flat_map(|r| r.permissions()).collect()
    }
}

/// Permissions that always require a human in the loop for autonomous agents.
pub const HUMAN_GATED: [Perm; 2] = [
    Perm(Permission::ExternalSideEffect),
    Perm(Permission::SelfModify),
];

/// A human approval token.  The signature is a hash over the token fields
/// and a server-side secret, so an agent cannot forge one; it is a
/// capability, not an identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HumanApproval {
    pub approver: String,
    /// Principal id the approval is issued for.
    pub subject: String,
    /// Permissions approved.
    pub scope: PermSet,
    pub expires_at: DateTime<Utc>,
    pub signature: String,
}

impl HumanApproval {
    fn payload(
        approver: &str,
        subject: &str,
        scope: &PermSet,
        expires_at: &DateTime<Utc>,
    ) -> String {
        let scope_s: Vec<String> = scope.iter().map(|p| p.to_string()).collect();
        format!(
            "{approver}|{subject}|{}|{}",
            scope_s.join(","),
            expires_at.to_rfc3339()
        )
    }

    /// Issue an approval.  `approver` must be a human administrator.
    pub fn issue(
        approver: &Principal,
        subject: &Principal,
        scope: impl IntoIterator<Item = Permission>,
        valid_for: Duration,
        signing_secret: &[u8],
    ) -> Result<Self> {
        if !approver.is_admin() {
            return Err(SimorphError::PermissionDenied(format!(
                "`{}` is not a human administrator and cannot issue approvals",
                approver.id
            )));
        }
        let scope = perm_set(scope);
        let expires_at = Utc::now() + valid_for;
        let payload = Self::payload(&approver.id, &subject.id, &scope, &expires_at);
        let signature = sign(&payload, signing_secret);
        Ok(Self {
            approver: approver.id.clone(),
            subject: subject.id.clone(),
            scope,
            expires_at,
            signature,
        })
    }

    /// Verify signature, expiry and subject.
    pub fn verify(&self, subject: &Principal, signing_secret: &[u8]) -> Result<()> {
        if self.subject != subject.id {
            return Err(SimorphError::PermissionDenied(
                "approval issued for another principal".into(),
            ));
        }
        if Utc::now() > self.expires_at {
            return Err(SimorphError::PermissionDenied("approval expired".into()));
        }
        let payload = Self::payload(&self.approver, &self.subject, &self.scope, &self.expires_at);
        if sign(&payload, signing_secret) != self.signature {
            return Err(SimorphError::PermissionDenied(
                "approval signature invalid".into(),
            ));
        }
        Ok(())
    }
}

fn sign(payload: &str, secret: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(secret.len() + payload.len() + 1);
    bytes.extend_from_slice(secret);
    bytes.push(0);
    bytes.extend_from_slice(payload.as_bytes());
    simorph_core::hashing::sha256_hex(&bytes)
}

/// Result of a successful authorisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Grant {
    pub principal: String,
    pub permissions: PermSet,
    pub approved_by: Option<String>,
    pub issued_at: DateTime<Utc>,
}

/// Authorisation policy.
pub struct Policy {
    signing_secret: Vec<u8>,
    audit: AuditLog,
}

impl Policy {
    pub fn new(signing_secret: impl Into<Vec<u8>>, audit: AuditLog) -> Self {
        Self {
            signing_secret: signing_secret.into(),
            audit,
        }
    }

    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    pub fn signing_secret(&self) -> &[u8] {
        &self.signing_secret
    }

    /// Authorise `requested` for `principal`.
    ///
    /// Rules, in order:
    /// 1. every requested permission must be held by one of the principal's roles,
    ///    unless covered by a valid human approval;
    /// 2. an `AutonomousAgent` can never hold a [`HUMAN_GATED`] permission from
    ///    its roles alone — only a valid [`HumanApproval`] can grant it, and
    ///    only for the permissions listed in its scope;
    /// 3. every decision is written to the audit log.
    pub fn authorize(
        &self,
        principal: &Principal,
        requested: impl IntoIterator<Item = Permission>,
        approval: Option<&HumanApproval>,
    ) -> Result<Grant> {
        let requested = perm_set(requested);
        let held = principal.role_permissions();
        let approved: PermSet = match approval {
            Some(a) => match a.verify(principal, &self.signing_secret) {
                Ok(()) => a.scope.clone(),
                Err(e) => {
                    self.audit.record(
                        principal,
                        &requested,
                        false,
                        &format!("invalid approval: {e}"),
                    );
                    return Err(e);
                }
            },
            None => BTreeSet::new(),
        };
        for p in &requested {
            let gated = HUMAN_GATED.contains(p);
            let agent = principal.kind == PrincipalKind::AutonomousAgent;
            let ok = if gated && agent {
                approved.contains(p)
            } else {
                held.contains(p) || approved.contains(p)
            };
            if !ok {
                let reason = if gated && agent {
                    format!("{p} requires a human approval for autonomous agents")
                } else {
                    format!("{p} not granted by roles {:?}", principal.roles)
                };
                self.audit.record(principal, &requested, false, &reason);
                return Err(SimorphError::PermissionDenied(reason));
            }
        }
        self.audit.record(principal, &requested, true, "granted");
        Ok(Grant {
            principal: principal.id.clone(),
            permissions: requested,
            approved_by: approval.map(|a| a.approver.clone()),
            issued_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy::new(b"test-secret".to_vec(), AuditLog::in_memory())
    }

    #[test]
    fn admin_cannot_be_auto_assigned() {
        assert!(Principal::new("x", PrincipalKind::AutonomousAgent, [Role::Admin]).is_err());
        assert!(Principal::new("x", PrincipalKind::User, [Role::Admin]).is_err());
        assert!(Principal::admin("root").is_admin());
    }

    #[test]
    fn agent_never_gets_side_effects_without_approval() {
        let p = policy();
        let agent = Principal::agent("director");
        assert!(p
            .authorize(
                &agent,
                [Permission::ReadWorkspace, Permission::SpawnProcess],
                None
            )
            .is_ok());
        assert!(p
            .authorize(&agent, [Permission::ExternalSideEffect], None)
            .is_err());
        assert!(p.authorize(&agent, [Permission::SelfModify], None).is_err());
        // Even an agent carrying every role except Admin is refused.
        let strong = Principal::new(
            "strong",
            PrincipalKind::AutonomousAgent,
            [Role::Agent, Role::Researcher, Role::Operator],
        )
        .unwrap();
        assert!(p
            .authorize(&strong, [Permission::SelfModify], None)
            .is_err());
        assert_eq!(p.audit().entries().iter().filter(|e| !e.granted).count(), 3);
    }

    #[test]
    fn valid_approval_grants_scoped_permission_only() {
        let p = policy();
        let admin = Principal::admin("alice");
        let agent = Principal::agent("director");
        let approval = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::SelfModify],
            Duration::minutes(5),
            p.signing_secret(),
        )
        .unwrap();
        let g = p
            .authorize(&agent, [Permission::SelfModify], Some(&approval))
            .unwrap();
        assert_eq!(g.approved_by.as_deref(), Some("alice"));
        // scope does not include ExternalSideEffect
        assert!(p
            .authorize(&agent, [Permission::ExternalSideEffect], Some(&approval))
            .is_err());
    }

    #[test]
    fn forged_expired_or_misdirected_approvals_are_rejected() {
        let p = policy();
        let admin = Principal::admin("alice");
        let agent = Principal::agent("director");
        let other = Principal::agent("other");
        let mut a = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::SelfModify],
            Duration::minutes(5),
            p.signing_secret(),
        )
        .unwrap();
        assert!(p
            .authorize(&other, [Permission::SelfModify], Some(&a))
            .is_err());
        a.scope.insert(Perm(Permission::ExternalSideEffect)); // tamper
        assert!(p
            .authorize(&agent, [Permission::SelfModify], Some(&a))
            .is_err());
        let expired = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::SelfModify],
            Duration::seconds(-1),
            p.signing_secret(),
        )
        .unwrap();
        assert!(p
            .authorize(&agent, [Permission::SelfModify], Some(&expired))
            .is_err());
        // non-admin cannot issue
        let researcher = Principal::new("bob", PrincipalKind::User, [Role::Researcher]).unwrap();
        assert!(HumanApproval::issue(
            &researcher,
            &agent,
            [Permission::SelfModify],
            Duration::minutes(1),
            b"s"
        )
        .is_err());
    }

    #[test]
    fn roles_bound_users_too() {
        let p = policy();
        let viewer = Principal::new("v", PrincipalKind::User, [Role::Viewer]).unwrap();
        assert!(p
            .authorize(&viewer, [Permission::WriteWorkspace], None)
            .is_err());
        assert!(p
            .authorize(&viewer, [Permission::ReadWorkspace], None)
            .is_ok());
    }
}
