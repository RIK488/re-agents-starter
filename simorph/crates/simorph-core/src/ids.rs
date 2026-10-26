//! Strongly typed identifiers.  Every experiment, run, model, hypothesis and
//! component carries a globally unique id so that provenance links are
//! unambiguous (§3.6, §11).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use uuid::Uuid;

/// A typed UUID.  The phantom marker prevents accidentally passing a
/// `RunId` where an `ExperimentId` is expected.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypedId<T> {
    uuid: Uuid,
    #[serde(skip)]
    _marker: PhantomData<fn() -> T>,
}

impl<T> JsonSchema for TypedId<T> {
    fn schema_name() -> String {
        "TypedId".to_string()
    }
    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <Uuid as JsonSchema>::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        false
    }
}

impl<T> TypedId<T> {
    pub fn new() -> Self {
        Self { uuid: Uuid::new_v4(), _marker: PhantomData }
    }
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self { uuid, _marker: PhantomData }
    }
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self::from_uuid)
    }
    /// Deterministic id derived from a name (UUID v5 in the SIMORPH namespace).
    /// Useful for reproducible fixtures and for content-addressed components.
    pub fn deterministic(name: &str) -> Self {
        const NS: Uuid = Uuid::from_bytes([
            0x5a, 0x1d, 0x0e, 0x3f, 0x9c, 0x4b, 0x4f, 0x2a, 0x8e, 0x1d, 0x6b, 0x7f, 0x3c, 0x2a,
            0x9d, 0x11,
        ]);
        Self::from_uuid(Uuid::new_v5(&NS, name.as_bytes()))
    }
    pub fn as_uuid(&self) -> Uuid {
        self.uuid
    }
}

impl<T> Default for TypedId<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Clone for TypedId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TypedId<T> {}
impl<T> PartialEq for TypedId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}
impl<T> Eq for TypedId<T> {}
impl<T> std::hash::Hash for TypedId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uuid.hash(state)
    }
}
impl<T> PartialOrd for TypedId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for TypedId<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.uuid.cmp(&other.uuid)
    }
}
impl<T> fmt::Debug for TypedId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}
impl<T> fmt::Display for TypedId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

macro_rules! id_type {
    ($(#[$m:meta])* $marker:ident, $alias:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $marker;
        pub type $alias = TypedId<$marker>;
    };
}

id_type!(#[doc = "Marker for problems."] ProblemMarker, ProblemId);
id_type!(#[doc = "Marker for models."] ModelMarker, ModelId);
id_type!(#[doc = "Marker for components (simulators, solvers, tools)."] ComponentMarker, ComponentId);
id_type!(#[doc = "Marker for experiments (a designed set of runs)."] ExperimentMarker, ExperimentId);
id_type!(#[doc = "Marker for a single execution run."] RunMarker, RunId);
id_type!(#[doc = "Marker for hypotheses."] HypothesisMarker, HypothesisId);
id_type!(#[doc = "Marker for validation records."] ValidationMarker, ValidationId);
id_type!(#[doc = "Marker for datasets."] DatasetMarker, DatasetId);
id_type!(#[doc = "Marker for scheduler tasks."] TaskMarker, TaskId);
id_type!(#[doc = "Marker for research campaigns driven by the scientific director."] CampaignMarker, CampaignId);
id_type!(#[doc = "Marker for AI improvement candidates."] CandidateMarker, CandidateId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_ids_are_stable() {
        let a = ComponentId::deterministic("physics.orbit.two-body");
        let b = ComponentId::deterministic("physics.orbit.two-body");
        let c = ComponentId::deterministic("physics.orbit.j2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn ids_round_trip_through_json() {
        let id = RunId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: RunId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
