use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::paths::VirtualPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEntry {
    pub virtual_path: VirtualPath,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub blob_id: Option<String>,
    pub encrypted_name: Option<crate::crypto::EncryptedPayload>,
}

impl VaultEntry {
    pub fn directory(virtual_path: VirtualPath) -> Self {
        Self {
            virtual_path,
            kind: EntryKind::Directory,
            size: 0,
            modified_at: None,
            blob_id: None,
            encrypted_name: None,
        }
    }

    pub fn file(
        virtual_path: VirtualPath,
        size: u64,
        modified_at: Option<DateTime<Utc>>,
        blob_id: String,
        encrypted_name: crate::crypto::EncryptedPayload,
    ) -> Self {
        Self {
            virtual_path,
            kind: EntryKind::File,
            size,
            modified_at,
            blob_id: Some(blob_id),
            encrypted_name: Some(encrypted_name),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultIndex {
    pub entries: BTreeMap<VirtualPath, VaultEntry>,
}

impl VaultIndex {
    pub fn insert(&mut self, entry: VaultEntry) {
        self.entries.insert(entry.virtual_path.clone(), entry);
    }

    pub fn get(&self, path: &VirtualPath) -> Option<&VaultEntry> {
        self.entries.get(path)
    }

    pub fn list(&self) -> impl Iterator<Item = &VaultEntry> {
        self.entries.values()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
