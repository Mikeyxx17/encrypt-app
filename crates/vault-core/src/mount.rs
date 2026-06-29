use std::path::Path;

use crate::error::{Result, VaultError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountStatus {
    Unmounted,
    Mounted { mount_point: String },
    Unsupported { reason: String },
}

pub trait MountProvider: Send + Sync {
    fn mount(&self, _vault_path: &Path, _mount_point: &Path) -> Result<MountStatus>;
    fn unmount(&self, _mount_point: &Path) -> Result<MountStatus>;
    fn status(&self) -> MountStatus;
}

#[derive(Debug, Clone, Default)]
pub struct NoopMountProvider;

impl MountProvider for NoopMountProvider {
    fn mount(&self, _vault_path: &Path, _mount_point: &Path) -> Result<MountStatus> {
        Err(VaultError::UnsupportedFeature(
            "virtual drive mounting is reserved for a later phase".to_string(),
        ))
    }

    fn unmount(&self, _mount_point: &Path) -> Result<MountStatus> {
        Ok(MountStatus::Unmounted)
    }

    fn status(&self) -> MountStatus {
        MountStatus::Unsupported {
            reason: "virtual drive mounting is reserved for a later phase".to_string(),
        }
    }
}
