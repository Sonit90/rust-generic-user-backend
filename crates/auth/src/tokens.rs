use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generate a 256-bit cryptographically random refresh token, URL-safe base64.
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    // No external base64 dep — use hex which is fine for an opaque token.
    hex::encode(bytes)
}

/// SHA-256 hash of the token, hex-encoded — never store the raw token.
pub fn hash_refresh_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}
