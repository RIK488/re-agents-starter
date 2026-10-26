//! Ordered wrapper around `simorph_core::Permission` (the core enum does not
//! derive `Ord`; this wrapper lets us use `BTreeSet` for deterministic,
//! serialisable permission sets).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::Permission;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Perm(pub Permission);

impl Perm {
    fn rank(&self) -> u8 {
        match self.0 {
            Permission::ReadWorkspace => 0,
            Permission::WriteWorkspace => 1,
            Permission::NetworkEgress => 2,
            Permission::SpawnProcess => 3,
            Permission::Gpu => 4,
            Permission::ReadSecrets => 5,
            Permission::ExternalSideEffect => 6,
            Permission::SelfModify => 7,
        }
    }
}

impl PartialOrd for Perm {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Perm {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}
impl From<Permission> for Perm {
    fn from(p: Permission) -> Self {
        Perm(p)
    }
}
impl std::fmt::Display for Perm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

/// Deterministic permission set.
pub type PermSet = BTreeSet<Perm>;

pub fn perm_set(items: impl IntoIterator<Item = Permission>) -> PermSet {
    items.into_iter().map(Perm).collect()
}
