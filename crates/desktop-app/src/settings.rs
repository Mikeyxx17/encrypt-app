use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use vault_core::{EntryKind, VaultEntry, VaultHandle};

use crate::formatting::{format_display_time, now_display_time};
#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalSettings {
    #[serde(default)]
    vaults: Vec<VaultRecord>,
    #[serde(default)]
    recent_vaults: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct VaultRecord {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) created_at: Option<String>,
    #[serde(default)]
    pub(crate) last_opened_at: Option<String>,
    #[serde(default)]
    pub(crate) file_count: Option<usize>,
    #[serde(default)]
    pub(crate) plaintext_bytes: Option<u64>,
    #[serde(default)]
    pub(crate) vault_size: Option<u64>,
    #[serde(default)]
    pub(crate) blob_count: Option<usize>,
    #[serde(default)]
    pub(crate) exists: bool,
}

pub(crate) fn remember_vault(
    records: &mut Vec<VaultRecord>,
    path: &Path,
    entries: &[VaultEntry],
    handle: &VaultHandle,
) {
    let path = path.display().to_string();
    let file_count = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .count();
    let plaintext_bytes = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.size)
        .sum::<u64>();

    records.retain(|record| record.path != path);
    let mut record = VaultRecord {
        path,
        created_at: Some(format_display_time(
            &handle.config().created_at.to_rfc3339(),
        )),
        last_opened_at: Some(now_display_time()),
        file_count: Some(file_count),
        plaintext_bytes: Some(plaintext_bytes),
        ..VaultRecord::default()
    };
    refresh_vault_record(&mut record);
    records.insert(0, record);
    records.truncate(20);
    save_vault_records(records);
}

pub(crate) fn load_vault_records() -> Vec<VaultRecord> {
    let Some(path) = settings_path() else {
        return Vec::new();
    };
    let Ok(data) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut settings = serde_json::from_str::<LocalSettings>(&data).unwrap_or_default();
    if settings.vaults.is_empty() && !settings.recent_vaults.is_empty() {
        settings.vaults = settings
            .recent_vaults
            .into_iter()
            .map(|path| VaultRecord {
                path,
                ..VaultRecord::default()
            })
            .collect();
    }
    refresh_vault_records(&mut settings.vaults);
    settings.vaults
}

pub(crate) fn save_vault_records(vaults: &[VaultRecord]) {
    let Some(path) = settings_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let settings = LocalSettings {
        vaults: vaults.to_vec(),
        recent_vaults: Vec::new(),
    };
    if let Ok(data) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, data);
    }
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("EncryptApp").join("settings.json"))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.join("encrypt-app-settings.json"))
        })
}

pub(crate) fn refresh_vault_records(records: &mut [VaultRecord]) {
    for record in records {
        refresh_vault_record(record);
    }
}

fn refresh_vault_record(record: &mut VaultRecord) {
    let root = PathBuf::from(&record.path);
    record.exists = root.exists();
    if !record.exists {
        record.vault_size = None;
        record.blob_count = None;
        return;
    }

    record.vault_size = directory_size(&root).ok();
    record.blob_count = count_files(&root.join("blobs")).ok();

    let config_path = root.join("vault.json");
    if let Ok(data) = fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str::<vault_core::VaultConfig>(&data) {
            record.created_at = Some(format_display_time(&config.created_at.to_rfc3339()));
        }
    }
}

fn directory_size(path: &Path) -> std::io::Result<u64> {
    let mut size = 0_u64;
    if !path.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            size = size.saturating_add(directory_size(&entry.path())?);
        } else {
            size = size.saturating_add(metadata.len());
        }
    }
    Ok(size)
}

fn count_files(path: &Path) -> std::io::Result<usize> {
    let mut count = 0_usize;
    if !path.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            count = count.saturating_add(count_files(&entry.path())?);
        } else {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

pub(crate) fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}
