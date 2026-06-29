use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{
    config::VAULT_CONFIG_FILE,
    crypto::{CryptoProvider, EncryptedPayload, KEY_LEN},
    error::{Result, VaultError},
    index::{EntryKind, VaultIndex},
};

const BLOBS_DIR: &str = "blobs";
const TEMP_EXTENSION: &str = "encrypt-app-tmp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHealthReport {
    pub vault_json_exists: bool,
    pub vault_json_valid: bool,
    pub index_decryptable: bool,
    pub total_files_in_index: usize,
    pub missing_blobs: Vec<MissingBlobEntry>,
    pub orphan_blobs: Vec<OrphanBlobEntry>,
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingBlobEntry {
    pub virtual_path: String,
    pub blob_id: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanBlobEntry {
    pub blob_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSummary {
    pub removed_count: usize,
    pub freed_bytes: u64,
}

const INDEX_FILE: &str = "index.enc";

pub fn check_health(root: &Path, index: &VaultIndex, key: &[u8; KEY_LEN]) -> VaultHealthReport {
    let index_decryptable = index_file_decryptable(root, key);
    build_health_report(root, Some(index), index_decryptable)
}

pub fn check_health_with_index_state(
    root: &Path,
    index: Option<&VaultIndex>,
    index_decryptable: bool,
) -> VaultHealthReport {
    build_health_report(root, index, index_decryptable)
}

fn build_health_report(
    root: &Path,
    index: Option<&VaultIndex>,
    index_decryptable: bool,
) -> VaultHealthReport {
    let vault_json_path = root.join(VAULT_CONFIG_FILE);
    let vault_json_exists = vault_json_path.exists();

    let vault_json_valid = if vault_json_exists {
        fs::read_to_string(&vault_json_path)
            .ok()
            .and_then(|data| serde_json::from_str::<crate::config::VaultConfig>(&data).ok())
            .is_some_and(|config| config.validate().is_ok())
    } else {
        false
    };

    let total_files_in_index = index
        .map(|index| {
            index
                .list()
                .filter(|entry| entry.kind == EntryKind::File)
                .count()
        })
        .unwrap_or(0);

    let mut missing_blobs = Vec::new();
    if let Some(index) = index {
        for entry in index.list().filter(|entry| entry.kind == EntryKind::File) {
            if let Some(blob_id) = &entry.blob_id {
                let blob_path = blob_path_from_id(root, blob_id);
                if !blob_path.exists() {
                    missing_blobs.push(MissingBlobEntry {
                        virtual_path: entry.virtual_path.to_string(),
                        blob_id: blob_id.clone(),
                        size: entry.size,
                    });
                }
            }
        }
    }

    let mut orphan_blobs = Vec::new();
    let mut reclaimable_bytes = 0_u64;
    if let Some(index) = index {
        let referenced: HashSet<&str> = index
            .list()
            .filter_map(|entry| entry.blob_id.as_deref())
            .collect();

        let blobs_root = root.join(BLOBS_DIR);
        if blobs_root.exists() {
            for entry in WalkDir::new(&blobs_root) {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
                        continue;
                    }
                    if path.extension().is_some_and(|ext| ext == TEMP_EXTENSION) {
                        continue;
                    }
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    if !referenced.contains(stem) {
                        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        orphan_blobs.push(OrphanBlobEntry {
                            blob_path: path.display().to_string(),
                            size,
                        });
                        reclaimable_bytes = reclaimable_bytes.saturating_add(size);
                    }
                }
            }
        }
    }

    VaultHealthReport {
        vault_json_exists,
        vault_json_valid,
        index_decryptable,
        total_files_in_index,
        missing_blobs,
        orphan_blobs,
        reclaimable_bytes,
    }
}

fn index_file_decryptable(root: &Path, key: &[u8; KEY_LEN]) -> bool {
    let index_path = root.join(INDEX_FILE);
    index_path.exists()
        && (|| {
            let data = fs::read(&index_path).map_err(|error| VaultError::io(&index_path, error))?;
            let encrypted: EncryptedPayload = serde_json::from_slice(&data)?;
            let plaintext = CryptoProvider::decrypt(key, &encrypted)?;
            serde_json::from_slice::<VaultIndex>(plaintext.as_slice())?;
            Ok::<bool, VaultError>(true)
        })()
        .unwrap_or(false)
}

pub fn cleanup_orphan_blobs_detailed(root: &Path, index: &VaultIndex) -> Result<CleanupSummary> {
    let referenced: HashSet<&str> = index
        .list()
        .filter_map(|entry| entry.blob_id.as_deref())
        .collect();

    let blobs_root = root.join(BLOBS_DIR);
    if !blobs_root.exists() {
        return Ok(CleanupSummary {
            removed_count: 0,
            freed_bytes: 0,
        });
    }

    let mut removed_count = 0_usize;
    let mut freed_bytes = 0_u64;

    for entry in WalkDir::new(&blobs_root) {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name.ends_with(&format!(".{TEMP_EXTENSION}")) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !referenced.contains(stem) {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            fs::remove_file(path).map_err(|error| VaultError::io(path, error))?;
            removed_count += 1;
            freed_bytes = freed_bytes.saturating_add(size);
        }
    }

    Ok(CleanupSummary {
        removed_count,
        freed_bytes,
    })
}

fn blob_path_from_id(root: &Path, blob_id: &str) -> PathBuf {
    let prefix = blob_id.get(0..2).unwrap_or("00");
    root.join(BLOBS_DIR)
        .join(prefix)
        .join(format!("{blob_id}.bin"))
}
