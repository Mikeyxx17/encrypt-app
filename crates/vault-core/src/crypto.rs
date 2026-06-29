use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    config::KdfConfig,
    error::{Result, VaultError},
};

pub const KEY_LEN: usize = 32;
pub const XCHACHA_NONCE_LEN: usize = 24;

pub type VaultKey = Zeroizing<[u8; KEY_LEN]>;
pub type Kek = Zeroizing<[u8; KEY_LEN]>;

const KEK_DOMAIN_SEPARATOR: &[u8] = b"zedrust.encrypt-app.kek";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

#[derive(Debug, Clone, Default)]
pub struct CryptoProvider;

impl CryptoProvider {
    pub fn derive_key(
        password: &SecretString,
        kdf: &KdfConfig,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let salt = kdf.salt()?;
        let params = Params::new(
            kdf.memory_cost_kib,
            kdf.time_cost,
            kdf.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|error| VaultError::Crypto(format!("invalid Argon2 params: {error}")))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        argon2
            .hash_password_into(password.expose_secret().as_bytes(), &salt, key.as_mut())
            .map_err(|error| VaultError::Crypto(format!("Argon2 failed: {error}")))?;
        Ok(key)
    }

    pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<EncryptedPayload> {
        let (nonce, ciphertext) = Self::encrypt_raw_with_aad(key, plaintext, &[])?;

        Ok(EncryptedPayload {
            nonce_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce),
            ciphertext_b64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                ciphertext,
            ),
        })
    }

    pub fn encrypt_raw_with_aad(
        key: &[u8; KEY_LEN],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<([u8; XCHACHA_NONCE_LEN], Vec<u8>)> {
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|error| VaultError::Crypto(format!("cipher init failed: {error}")))?;
        let mut nonce = [0_u8; XCHACHA_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| VaultError::Crypto("encryption failed".to_string()))?;

        Ok((nonce, ciphertext))
    }

    pub fn decrypt(key: &[u8; KEY_LEN], payload: &EncryptedPayload) -> Result<Zeroizing<Vec<u8>>> {
        let nonce = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.nonce_b64,
        )
        .map_err(|error| VaultError::InvalidFormat(format!("invalid nonce: {error}")))?;
        if nonce.len() != XCHACHA_NONCE_LEN {
            return Err(VaultError::InvalidFormat(format!(
                "nonce must be {XCHACHA_NONCE_LEN} bytes"
            )));
        }

        let mut ciphertext = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.ciphertext_b64,
        )
        .map_err(|error| VaultError::InvalidFormat(format!("invalid ciphertext: {error}")))?;
        let plaintext = Self::decrypt_raw_with_aad(key, &nonce, &ciphertext, &[])?;
        ciphertext.zeroize();
        Ok(Zeroizing::new(plaintext))
    }

    pub fn decrypt_raw_with_aad(
        key: &[u8; KEY_LEN],
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        if nonce.len() != XCHACHA_NONCE_LEN {
            return Err(VaultError::InvalidFormat(format!(
                "nonce must be {XCHACHA_NONCE_LEN} bytes"
            )));
        }

        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|error| VaultError::Crypto(format!("cipher init failed: {error}")))?;
        cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| VaultError::TamperedCiphertext)
    }

    pub fn derive_kek(password: &SecretString, kdf: &KdfConfig) -> Result<Kek> {
        let salt = kdf.salt()?;
        let params = Params::new(
            kdf.memory_cost_kib,
            kdf.time_cost,
            kdf.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|error| VaultError::Crypto(format!("invalid Argon2 params: {error}")))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);

        let mut password_with_domain = password.expose_secret().as_bytes().to_vec();
        password_with_domain.extend_from_slice(KEK_DOMAIN_SEPARATOR);

        argon2
            .hash_password_into(&password_with_domain, &salt, key.as_mut())
            .map_err(|error| VaultError::Crypto(format!("Argon2 KEK failed: {error}")))?;

        password_with_domain.zeroize();
        Ok(key)
    }

    pub fn generate_vault_key() -> VaultKey {
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        OsRng.fill_bytes(key.as_mut());
        key
    }

    pub fn wrap_vault_key(vault_key: &VaultKey, kek: &Kek) -> Result<EncryptedPayload> {
        Self::encrypt(kek, vault_key.as_slice())
    }

    pub fn unwrap_vault_key(wrapped: &EncryptedPayload, kek: &Kek) -> Result<VaultKey> {
        let plaintext = Self::decrypt(kek, wrapped)
            .map_err(|error| VaultError::VaultKeyUnwrapFailed(error.to_string()))?;
        if plaintext.len() != KEY_LEN {
            return Err(VaultError::VaultKeyUnwrapFailed(format!(
                "expected {KEY_LEN} bytes, got {}",
                plaintext.len()
            )));
        }
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        key.as_mut().copy_from_slice(plaintext.as_slice());
        Ok(key)
    }
}
