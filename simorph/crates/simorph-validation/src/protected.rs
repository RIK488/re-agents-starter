//! Sealed reference sets.
//!
//! The scientific director may *use* references to judge its results but
//! must never be able to change them (§3.2, §5.2).  A `ProtectedReferences`
//! set is mutable until sealed; afterwards every mutation returns
//! `SimorphError::Protected` and the seal hash is embedded in each
//! validation record so that a swapped reference set is detectable.

use crate::classify::ReferenceOrigin;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{hashing::hash_json, ReferenceCase, Result, SimorphError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectedCase {
    pub case: ReferenceCase,
    pub origin: ReferenceOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectedReferences {
    pub name: String,
    cases: Vec<ProtectedCase>,
    seal: Option<String>,
}

impl ProtectedReferences {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: vec![],
            seal: None,
        }
    }

    fn guard(&self) -> Result<()> {
        match &self.seal {
            Some(h) => Err(SimorphError::Protected(format!(
                "reference set `{}` is sealed ({h})",
                self.name
            ))),
            None => Ok(()),
        }
    }

    pub fn add(&mut self, case: ReferenceCase, origin: ReferenceOrigin) -> Result<()> {
        self.guard()?;
        self.cases.push(ProtectedCase { case, origin });
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.guard()?;
        self.cases.retain(|c| c.case.name != name);
        Ok(())
    }

    /// Seal the set: computes and stores the hash; mutations are refused
    /// from now on.  Sealing an already-sealed set is a no-op.
    pub fn seal(&mut self) -> String {
        if let Some(h) = &self.seal {
            return h.clone();
        }
        let h = hash_json(&self.cases).expect("serialisable");
        self.seal = Some(h.clone());
        h
    }

    pub fn is_sealed(&self) -> bool {
        self.seal.is_some()
    }

    pub fn seal_hash(&self) -> Option<&str> {
        self.seal.as_deref()
    }

    /// Recompute the hash and compare with the stored seal.  Detects a set
    /// that was deserialised from a tampered file.
    pub fn verify(&self) -> Result<()> {
        match &self.seal {
            None => Err(SimorphError::Validation(format!(
                "reference set `{}` is not sealed",
                self.name
            ))),
            Some(h) => {
                let now = hash_json(&self.cases).expect("serialisable");
                if &now == h {
                    Ok(())
                } else {
                    Err(SimorphError::Protected(format!(
                        "reference set `{}` has been tampered with",
                        self.name
                    )))
                }
            }
        }
    }

    pub fn cases(&self) -> &[ProtectedCase] {
        &self.cases
    }

    pub fn reference_cases(&self) -> Vec<ReferenceCase> {
        self.cases.iter().map(|c| c.case.clone()).collect()
    }

    pub fn origins(&self) -> Vec<ReferenceOrigin> {
        self.cases.iter().map(|c| c.origin).collect()
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Load and verify a sealed set.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let s: Self = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        s.verify()?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(n: &str) -> ReferenceCase {
        ReferenceCase {
            name: n.into(),
            inputs: serde_json::json!({"x": 1}),
            expected_outputs: serde_json::json!({"y": 2}),
            rtol: 1e-6,
            atol: 0.0,
            source: "lab".into(),
        }
    }

    #[test]
    fn sealed_set_refuses_modification_and_detects_tampering() {
        let mut p = ProtectedReferences::new("orbit-refs");
        p.add(case("a"), ReferenceOrigin::Empirical).unwrap();
        let h = p.seal();
        assert_eq!(p.seal(), h);
        assert!(matches!(
            p.add(case("b"), ReferenceOrigin::Analytic),
            Err(SimorphError::Protected(_))
        ));
        assert!(matches!(p.remove("a"), Err(SimorphError::Protected(_))));
        assert!(p.verify().is_ok());
        // simulate tampering through deserialisation
        let mut json: serde_json::Value = serde_json::to_value(&p).unwrap();
        json["cases"][0]["case"]["expected_outputs"]["y"] = serde_json::json!(3);
        let tampered: ProtectedReferences = serde_json::from_value(json).unwrap();
        assert!(matches!(tampered.verify(), Err(SimorphError::Protected(_))));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("refs.json");
        p.save(&path).unwrap();
        assert!(ProtectedReferences::load(&path).is_ok());
        tampered.save(&path).unwrap();
        assert!(ProtectedReferences::load(&path).is_err());
    }
}
