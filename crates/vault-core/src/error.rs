use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("the password is incorrect or the vault index cannot be authenticated")]
    InvalidPassword,
    #[error("encrypted data failed authentication")]
    TamperedCiphertext,
    #[error("vault format version {found} is not supported")]
    UnsupportedVersion { found: u32 },
    #[error("invalid vault format: {0}")]
    InvalidFormat(String),
    #[error("invalid virtual path: {0}")]
    InvalidPath(String),
    #[error("path is not inside the imported root: {path}")]
    PathOutsideRoot { path: PathBuf },
    #[error("vault already exists at {path}")]
    VaultAlreadyExists { path: PathBuf },
    #[error("vault is already open or locked: {path}")]
    VaultLocked { path: PathBuf },
    #[error("encrypted blob is missing for {virtual_path}: {blob_id}")]
    MissingBlob {
        virtual_path: String,
        blob_id: String,
    },
    #[error("the folder is not empty and is not a vault: {path}")]
    DirectoryNotEmpty { path: PathBuf },
    #[error("import source must be outside the vault folder: {path}")]
    ImportSourceInsideVault { path: PathBuf },
    #[error("export destination must be outside the vault folder: {destination}")]
    ExportDestinationInsideVault { destination: PathBuf },
    #[error("operation was cancelled")]
    OperationCancelled,
    #[error(
        "not enough disk space at {path}: required {required} bytes, available {available} bytes"
    )]
    InsufficientSpace {
        path: PathBuf,
        required: u64,
        available: u64,
    },
    #[error("vault key unwrap failed: {0}")]
    VaultKeyUnwrapFailed(String),
    #[error("feature is not supported yet: {0}")]
    UnsupportedFeature(String),
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
}

impl VaultError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;
