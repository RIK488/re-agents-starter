//! Content hashing helpers used for provenance and protected references.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// SHA-256 hex digest of raw bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// SHA-256 of the *canonical* JSON encoding of a value.  `serde_json`
/// preserves struct field order, and SIMORPH uses `IndexMap`/`BTreeMap` for
/// maps, so the encoding is deterministic for the same logical value.
pub fn hash_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
