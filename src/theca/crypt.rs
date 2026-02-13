use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use argon2::{
    password_hash::{
        rand_core::RngCore,
    },
    Argon2
};
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum CryptError {
    Encryption,
    Decryption,
    KeyDerivation,
}

impl fmt::Display for CryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptError::Encryption => write!(f, "Encryption failed"),
            CryptError::Decryption => write!(f, "Decryption failed"),
            CryptError::KeyDerivation => write!(f, "Key derivation failed"),
        }
    }
}

impl Error for CryptError {}

/// Derives a 32-byte key from a password and salt using Argon2id.
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], CryptError> {
    let mut key = [0u8; 32];
    let argon2 = Argon2::default();
    
    // We need to output raw bytes for the key, but the `password_hash` crate 
    // focuses on PHC string format. We can use `hash_password_custom` into output.
    // However, simpler is often better. Let's use `argon2` simplified API 
    // or just standard usage.
    
    // Actually, to get raw key bytes for encryption (not password verification),
    // we should use a KDF mode or just hash it. 
    // Argon2id is suitable.
    
    // Crates often change APIs. simpler `Argon2::default().hash_password` returns a `PasswordHash`.
    // We can't easily extract raw bytes from `PasswordHash` for encryption key usage 
    // (it's designed for storage).
    
    // Let's use `Argon2::hash_password_into`.
    argon2.hash_password_into(
        password.as_bytes(), 
        salt, 
        &mut key
    ).map_err(|_| CryptError::KeyDerivation)?;

    Ok(key)
}

/// Encrypts data using XChaCha20Poly1305.
/// Returns a vector containing [Salt (16 bytes) | Nonce (24 bytes) | Ciphertext].
pub fn encrypt(data: &[u8], password: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let key_bytes = derive_key(password, &salt)?;
    let key = chacha20poly1305::Key::from_slice(&key_bytes);
    let cipher = XChaCha20Poly1305::new(key);

    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng); // 24-bytes; unique per message
    
    let ciphertext = cipher.encrypt(&nonce, data)
        .map_err(|_| CryptError::Encryption)?;

    let mut result = Vec::with_capacity(16 + 24 + ciphertext.len());
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypts data using XChaCha20Poly1305.
/// Expects data format: [Salt (16 bytes) | Nonce (24 bytes) | Ciphertext].
pub fn decrypt(encrypted_data: &[u8], password: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if encrypted_data.len() < 16 + 24 {
        return Err(Box::new(CryptError::Decryption));
    }

    let salt = &encrypted_data[0..16];
    let nonce = XNonce::from_slice(&encrypted_data[16..40]);
    let ciphertext = &encrypted_data[40..];

    let key_bytes = derive_key(password, salt)?;
    let key = chacha20poly1305::Key::from_slice(&key_bytes);
    let cipher = XChaCha20Poly1305::new(key);

    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| CryptError::Decryption)?;

    Ok(plaintext)
}
