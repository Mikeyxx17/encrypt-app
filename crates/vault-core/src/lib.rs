pub mod backend;
pub mod config;
pub mod crypto;
pub mod error;
pub mod health;
pub mod index;
pub mod mount;
pub mod paths;

pub use backend::{
    CustomVaultBackend, FailurePolicy, ImportConflictPolicy, ImportOptions, ImportSummary,
    OperationControl, OperationIssue, OperationKind, OperationProgress, OperationReport,
    VaultBackend, VaultHandle,
};
pub use config::{
    KdfConfig, VaultConfig, CURRENT_FORMAT_VERSION, VAULT_CONFIG_FILE, VAULT_FORMAT_ID,
    VAULT_VERSION,
};
pub use crypto::{CryptoProvider, EncryptedPayload, Kek, VaultKey};
pub use error::{Result, VaultError};
pub use health::{
    check_health, check_health_with_index_state, cleanup_orphan_blobs_detailed, CleanupSummary,
    MissingBlobEntry, OrphanBlobEntry, VaultHealthReport,
};
pub use index::{EntryKind, VaultEntry, VaultIndex};
pub use mount::{MountProvider, MountStatus, NoopMountProvider};
pub use paths::VirtualPath;
