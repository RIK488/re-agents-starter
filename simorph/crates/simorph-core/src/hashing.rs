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

/// HMAC-SHA256 (RFC 2104) of `msg` under `key`, hex-encoded.  Used for
/// keyed seals of provenance ledgers and protected references: without the
/// key, an attacker who edits the data cannot produce a matching MAC.
pub fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time equality of two strings (MAC comparison).
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_rfc4231_case_2() {
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert!(ct_eq("ab", "ab") && !ct_eq("ab", "ac") && !ct_eq("a", "ab"));
    }

    #[test]
    fn known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
