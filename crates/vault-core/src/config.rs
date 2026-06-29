use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::error::{Result, VaultError};

pub const VAULT_VERSION: u32 = 1;
pub const CURRENT_FORMAT_VERSION: u32 = 2;
pub const VAULT_FORMAT_ID: &str = "zedrust.encrypt-app.custom-vault";
pub const VAULT_CONFIG_FILE: &str = "vault.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfConfig {
    pub algorithm: String,
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub output_len: usize,
    pub salt_b64: String,
}

impl KdfConfig {
    pub fn new_interactive() -> Self {
        let mut salt = [0_u8; 32];
        OsRng.fill_bytes(&mut salt);

        Self {
            algorithm: "argon2id".to_string(),
            memory_cost_kib: 64 * 1024,
            time_cost: 3,
            parallelism: 1,
            output_len: 32,
            salt_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, salt),
        }
    }

    pub fn salt(&self) -> Result<Vec<u8>> {
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &self.salt_b64)
            .map_err(|error| VaultError::InvalidFormat(format!("invalid KDF salt: {error}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultConfig {
    pub format_id: String,
    pub version: u32,
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub created_at: DateTime<Utc>,
    pub kdf: KdfConfig,
    #[serde(default)]
    pub kdf_salt: String,
    #[serde(default)]
    pub wrapped_vault_key: String,
    #[serde(default)]
    pub wrapped_key_nonce: String,
    #[serde(default)]
    pub password_changed_at: Option<DateTime<Utc>>,
}

fn default_format_version() -> u32 {
    CURRENT_FORMAT_VERSION
}

impl VaultConfig {
    pub fn new() -> Self {
        let mut kdf_salt_bytes = [0_u8; 16];
        OsRng.fill_bytes(&mut kdf_salt_bytes);
        let kdf_salt =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, kdf_salt_bytes);

        Self {
            format_id: VAULT_FORMAT_ID.to_string(),
            version: VAULT_VERSION,
            format_version: CURRENT_FORMAT_VERSION,
            created_at: Utc::now(),
            kdf: KdfConfig::new_interactive(),
            kdf_salt,
            wrapped_vault_key: String::new(),
            wrapped_key_nonce: String::new(),
            password_changed_at: None,
        }
    }

    pub fn effective_format_version(&self) -> u32 {
        if self.format_version == 0 {
            self.version
        } else {
            self.format_version
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_id != VAULT_FORMAT_ID {
            return Err(VaultError::InvalidFormat(format!(
                "unexpected format id '{}'",
                self.format_id
            )));
        }

        if self.version != VAULT_VERSION {
            return Err(VaultError::UnsupportedVersion {
                found: self.version,
            });
        }

        if self.kdf.algorithm != "argon2id" {
            return Err(VaultError::InvalidFormat(format!(
                "unsupported KDF '{}'",
                self.kdf.algorithm
            )));
        }

        Ok(())
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self::new()
    }
}
