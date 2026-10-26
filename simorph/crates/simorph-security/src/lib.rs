//! # simorph-security
//!
//! Security model of SIMORPH (§10.4, §5.2):
//!
//! * [`rbac`] – principals, roles and the authorisation policy.  Autonomous
//!   agents can never obtain `ExternalSideEffect` or `SelfModify` without a
//!   [`rbac::HumanApproval`] token; the rule is hard-coded, not configurable.
//! * [`sandbox`] – declarative sandbox policy for generated / external code.
//! * [`secrets`] – allow-listed secret store and log redactor.
//! * [`protected`] – sealed, hash-verified resources (validation references,
//!   baselines) that only an administrator with human approval may change.
//! * [`audit`] – append-only log of every authorisation decision.

pub mod audit;
pub mod perm;
pub mod protected;
pub mod rbac;
pub mod sandbox;
pub mod secrets;

pub use audit::{AuditEntry, AuditLog};
pub use perm::{perm_set, Perm, PermSet};
pub use protected::ProtectedGuard;
pub use rbac::{Grant, HumanApproval, Policy, Principal, PrincipalKind, Role};
pub use sandbox::SandboxPolicy;
pub use secrets::{Redactor, SecretStore};
