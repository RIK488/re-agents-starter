//! Shared-secret HMAC-SHA256 challenge/response.
//!
//! The coordinator sends a random nonce; the worker answers
//! `hex(HMAC(secret, nonce || "|" || worker_id))`.  The secret never travels.
//! This authenticates workers; it does not encrypt traffic (use a TLS tunnel
//! or a private network for confidentiality).

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn respond(secret: &[u8], nonce: &str, worker_id: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("any key length");
    mac.update(nonce.as_bytes());
    mac.update(b"|");
    mac.update(worker_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time verification.
pub fn verify(secret: &[u8], nonce: &str, worker_id: &str, response: &str) -> bool {
    let Ok(bytes) = hex::decode(response) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("any key length");
    mac.update(nonce.as_bytes());
    mac.update(b"|");
    mac.update(worker_id.as_bytes());
    mac.verify_slice(&bytes).is_ok()
}

pub fn nonce() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_round_trip_and_rejections() {
        let n = nonce();
        let r = respond(b"s3cret", &n, "w1");
        assert!(verify(b"s3cret", &n, "w1", &r));
        assert!(!verify(b"other", &n, "w1", &r));
        assert!(!verify(b"s3cret", &n, "w2", &r));
        assert!(!verify(b"s3cret", &nonce(), "w1", &r));
        assert!(!verify(b"s3cret", &n, "w1", "zz"));
    }
}
