//! Crypto module — Argon2id key derivation + XChaCha20-Poly1305 encryption.
//!
//! Design:
//! - Vault key is 32 bytes derived from user password via Argon2id.
//! - The Argon2id salt (16 bytes) is stored on disk next to the DB.
//! - Encrypted records are stored as: nonce (24 bytes) || ciphertext || tag.
//! - The key is NEVER written to disk.
//!
//! See SECURITY.md for full threat model.

use argon2::{Argon2, Params, Algorithm, Version};
use chacha20poly1305::{
    XChaCha20Poly1305, Key, XNonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use rand::rngs::OsRng;

use crate::error::{AppError, AppResult};

/// Argon2id parameters — tuned for desktop (interactive) use.
const ARGON2_M_COST: u32 = 65536; // 64 MB
const ARGON2_T_COST: u32 = 3;     // 3 iterations
const ARGON2_P_COST: u32 = 4;     // 4 parallelism
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

/// Holds the in-memory vault key. Zeroed on drop.
pub struct CryptoContext {
    key: [u8; KEY_LEN],
}

impl CryptoContext {
    /// Derive a vault key from `password` and `salt` using Argon2id.
    pub fn derive(password: &str, salt: &[u8]) -> AppResult<Self> {
        let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(KEY_LEN))
            .map_err(|e| AppError::Crypto(e.to_string()))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = [0u8; KEY_LEN];
        argon2
            .hash_password_into(password.as_bytes(), salt, &mut key)
            .map_err(|e| AppError::Crypto(e.to_string()))?;
        Ok(Self { key })
    }

    /// Encrypt `plaintext`. Returns `nonce || ciphertext_with_tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.key));
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AppError::Crypto(e.to_string()))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt data previously produced by `encrypt`.
    pub fn decrypt(&self, data: &[u8]) -> AppResult<Vec<u8>> {
        if data.len() < NONCE_LEN + 16 {
            return Err(AppError::Crypto("Ciphertext too short".into()));
        }
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.key));
        let nonce = XNonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| AppError::Crypto("Decryption failed — wrong key or corrupted data".into()))
    }

    /// Convenience: encrypt a UTF-8 string.
    pub fn encrypt_str(&self, plaintext: &str) -> AppResult<Vec<u8>> {
        self.encrypt(plaintext.as_bytes())
    }

    /// Convenience: decrypt to a UTF-8 string.
    pub fn decrypt_str(&self, data: &[u8]) -> AppResult<String> {
        let bytes = self.decrypt(data)?;
        String::from_utf8(bytes).map_err(|_| AppError::Crypto("Decrypted bytes are not valid UTF-8".into()))
    }

    /// Expose raw key bytes for sharing with background threads (e.g. the Telegram bot).
    /// The caller must store and treat these bytes with the same care as the full CryptoContext.
    pub fn key_bytes(&self) -> [u8; KEY_LEN] {
        self.key
    }

    /// Reconstruct a CryptoContext from raw key bytes previously obtained via `key_bytes()`.
    pub fn from_key_bytes(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }
}

// Zero the key on drop — prevents it lingering in memory after the vault is locked.
impl Drop for CryptoContext {
    fn drop(&mut self) {
        self.key.iter_mut().for_each(|b| *b = 0);
    }
}

/// Generate a fresh random salt for Argon2.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let salt = generate_salt();
        let ctx = CryptoContext::derive("hunter2", &salt).unwrap();
        let plaintext = b"Hello, encrypted world!";
        let encrypted = ctx.encrypt(plaintext).unwrap();
        let decrypted = ctx.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_key_fails() {
        let salt = generate_salt();
        let ctx1 = CryptoContext::derive("password1", &salt).unwrap();
        let ctx2 = CryptoContext::derive("password2", &salt).unwrap();
        let encrypted = ctx1.encrypt(b"secret").unwrap();
        assert!(ctx2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_nonces_are_unique() {
        let salt = generate_salt();
        let ctx = CryptoContext::derive("pw", &salt).unwrap();
        let e1 = ctx.encrypt(b"same data").unwrap();
        let e2 = ctx.encrypt(b"same data").unwrap();
        // Nonces must differ (probabilistically certain with 24-byte random nonces)
        assert_ne!(&e1[..24], &e2[..24]);
    }

    #[test]
    fn test_string_roundtrip() {
        let salt = generate_salt();
        let ctx = CryptoContext::derive("vault_password", &salt).unwrap();
        let original = "This is a test memory.";
        let enc = ctx.encrypt_str(original).unwrap();
        let dec = ctx.decrypt_str(&enc).unwrap();
        assert_eq!(original, dec);
    }
}
