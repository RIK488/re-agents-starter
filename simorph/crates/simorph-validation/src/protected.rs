//! Sealed reference sets.
//!
//! The scientific director may *use* references to judge its results but
//! must never be able to change them (§3.2, §5.2).  A `ProtectedReferences`
//! set is mutable until sealed; afterwards every mutation returns
//! `SimorphError::Protected` and the seal hash is embedded in each
//! validation record so that a swapped reference set is detectable.
//!
//! Two seal modes:
//! * unkeyed ([`ProtectedReferences::seal`]) — a plain content hash. It
//!   detects accidental or naive edits only: anyone able to rewrite the file
//!   can recompute the hash.
//! * keyed ([`ProtectedReferences::seal_keyed`]) — an HMAC-SHA256 under a key
//!   held by the policy owner; edit-and-rehash is detected because the
//!   attacker cannot recompute the MAC without the key.

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

    /// Seal with an HMAC-SHA256 under `key` (stored as `hmac:<hex>`).
    pub fn seal_keyed(&mut self, key: &[u8]) -> String {
        if let Some(h) = &self.seal {
            return h.clone();
        }
        let body = serde_json::to_vec(&self.cases).expect("serialisable");
        let h = format!(
            "hmac:{}",
            simorph_core::hashing::hmac_sha256_hex(key, &body)
        );
        self.seal = Some(h.clone());
        h
    }

    /// Verify a keyed seal.
    pub fn verify_keyed(&self, key: &[u8]) -> Result<()> {
        let Some(h) = &self.seal else {
            return Err(SimorphError::Validation(format!(
                "reference set `{}` is not sealed",
                self.name
            )));
        };
        let body = serde_json::to_vec(&self.cases)?;
        let expected = format!(
            "hmac:{}",
            simorph_core::hashing::hmac_sha256_hex(key, &body)
        );
        if simorph_core::hashing::ct_eq(&expected, h) {
            Ok(())
        } else {
            Err(SimorphError::Protected(format!(
                "reference set `{}`: keyed seal mismatch (tampered or wrong key)",
                self.name
            )))
        }
    }

    /// Load and verify a set sealed with [`ProtectedReferences::seal_keyed`].
    pub fn load_keyed(path: impl AsRef<std::path::Path>, key: &[u8]) -> Result<Self> {
        let s: Self = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        s.verify_keyed(key)?;
        Ok(s)
    }

    /// Origins indexed by case name (input of `classify_with_origins`).
    pub fn origins_by_case(&self) -> indexmap::IndexMap<String, ReferenceOrigin> {
        self.cases
            .iter()
            .map(|c| (c.case.name.clone(), c.origin))
            .collect()
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
            Some(h) if h.starts_with("hmac:") => Err(SimorphError::Protected(format!(
                "reference set `{}` has a keyed seal: use verify_keyed",
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

    #[test]
    fn audit_keyed_seal_detects_edit_and_rehash() {
        // Audit probe: editing expected value + rtol and recomputing the
        // unkeyed hash made `load` accept the file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("refs.json");
        let key = b"policy-owner-key";
        let mut r = ProtectedReferences::new("refs");
        r.add(case("a"), ReferenceOrigin::Empirical).unwrap();
        r.seal_keyed(key);
        r.save(&path).unwrap();
        ProtectedReferences::load_keyed(&path, key).unwrap();
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        v["cases"][0]["case"]["expected_outputs"]["y"] = serde_json::json!(3);
        v["cases"][0]["case"]["rtol"] = serde_json::json!(1.0);
        let cases = v["cases"].clone();
        let cases: Vec<ProtectedCase> = serde_json::from_value(cases).unwrap();
        v["seal"] = serde_json::json!(hash_json(&cases).unwrap());
        std::fs::write(&path, v.to_string()).unwrap();
        assert!(ProtectedReferences::load_keyed(&path, key).is_err());
        // The unkeyed loader is fooled by construction — documented limit.
        assert!(ProtectedReferences::load(&path).is_ok());
        assert_eq!(r.origins_by_case()["a"], ReferenceOrigin::Empirical);
    }
}
