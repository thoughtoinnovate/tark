//! Simple passphrase-based encryption for sensitive local credentials.
//!
//! Uses Argon2id for key derivation and ChaCha20-Poly1305 for AEAD.

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use once_cell::sync::OnceCell;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::Path;

const VERSION: u8 = 1;
const KDF: &str = "argon2id";
const ENV_PASSPHRASE: &str = "TARK_PLUGIN_PASSPHRASE";

static PASSPHRASE_CACHE: OnceCell<String> = OnceCell::new();

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedPayload {
    version: u8,
    kdf: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

pub fn encrypt_file_in_place(path: &Path, passphrase: &str) -> Result<()> {
    let plaintext = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let encrypted = encrypt_string(&plaintext, passphrase)?;
    std::fs::write(path, encrypted)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn read_maybe_encrypted(path: &Path) -> Result<String> {
    let payload = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if let Ok(enc) = serde_json::from_str::<EncryptedPayload>(&payload) {
        if enc.version == VERSION && enc.kdf == KDF {
            let passphrase = passphrase_for_read()?;
            return decrypt_string(&payload, &passphrase);
        }
    }
    Ok(payload)
}

pub fn encrypt_string(plaintext: &str, passphrase: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;

    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|err| anyhow!("Encryption failed: {}", err))?;

    let payload = EncryptedPayload {
        version: VERSION,
        kdf: KDF.to_string(),
        salt: STANDARD_NO_PAD.encode(salt),
        nonce: STANDARD_NO_PAD.encode(nonce_bytes),
        ciphertext: STANDARD_NO_PAD.encode(ciphertext),
    };

    Ok(serde_json::to_string_pretty(&payload)?)
}

pub fn decrypt_string(payload: &str, passphrase: &str) -> Result<String> {
    let enc: EncryptedPayload = serde_json::from_str(payload)?;
    let salt = STANDARD_NO_PAD
        .decode(enc.salt)
        .context("Invalid salt encoding")?;
    let nonce = STANDARD_NO_PAD
        .decode(enc.nonce)
        .context("Invalid nonce encoding")?;
    let ciphertext = STANDARD_NO_PAD
        .decode(enc.ciphertext)
        .context("Invalid ciphertext encoding")?;

    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|err| anyhow!("Decryption failed (bad passphrase?): {}", err))?;

    Ok(String::from_utf8(plaintext)?)
}

pub fn prompt_new_passphrase() -> Result<String> {
    if let Ok(pass) = std::env::var(ENV_PASSPHRASE) {
        return Ok(pass);
    }
    let first = rpassword::prompt_password("New passphrase: ")?;
    let second = rpassword::prompt_password("Confirm passphrase: ")?;
    if first != second {
        anyhow::bail!("Passphrases do not match");
    }
    PASSPHRASE_CACHE.set(first.clone()).ok();
    Ok(first)
}

fn passphrase_for_read() -> Result<String> {
    if let Ok(pass) = std::env::var(ENV_PASSPHRASE) {
        return Ok(pass);
    }
    if let Some(pass) = PASSPHRASE_CACHE.get() {
        return Ok(pass.clone());
    }
    let pass = rpassword::prompt_password("Passphrase: ")?;
    PASSPHRASE_CACHE.set(pass.clone()).ok();
    Ok(pass)
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    let params = argon2::Params::new(19456, 2, 1, Some(32))
        .map_err(|err| anyhow!("Invalid Argon2 parameters: {}", err))?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|err| anyhow!("KDF failed: {}", err))?;
    Ok(key)
}
