use std::{
    collections::HashSet,
    fs,
    io::{BufReader, BufWriter, ErrorKind, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use rand::{rngs::OsRng, RngCore};
use secrecy::SecretString;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zeroize::Zeroizing;

use crate::{
    config::{KdfConfig, VaultConfig, CURRENT_FORMAT_VERSION, VAULT_CONFIG_FILE},
    crypto::{CryptoProvider, EncryptedPayload, VaultKey, KEY_LEN, XCHACHA_NONCE_LEN},
    error::{Result, VaultError},
    index::{EntryKind, VaultEntry, VaultIndex},
    paths::VirtualPath,
};

const INDEX_FILE: &str = "index.enc";
const INDEX_TEMP_FILE: &str = "index.enc.tmp";
const INDEX_BACKUP_FILE: &str = "index.enc.bak";
const BLOBS_DIR: &str = "blobs";
const LOCK_FILE: &str = "vault.lock";
const CHUNK_SIZE: usize = 1024 * 1024;
const BINARY_BLOB_MAGIC: &[u8; 8] = b"ZEVBLOB2";
const BINARY_BLOB_VERSION: u16 = 2;
const BLOB_AAD_DOMAIN: &[u8] = b"zedrust-vault-blob-v2";
const TEMP_EXTENSION: &str = "encrypt-app-tmp";
static ACTIVE_VAULT_LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

pub trait VaultBackend {
    fn create(path: impl AsRef<Path>, password: SecretString) -> Result<VaultHandle>;
    fn open(path: impl AsRef<Path>, password: SecretString) -> Result<VaultHandle>;
}

#[derive(Debug, Clone, Default)]
pub struct CustomVaultBackend;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImportConflictPolicy {
    Overwrite,
    Skip,
    #[default]
    Rename,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportOptions {
    pub conflict_policy: ImportConflictPolicy,
    pub failure_policy: FailurePolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OperationKind {
    #[default]
    Idle,
    Import,
    Export,
}

#[derive(Debug, Clone, Default)]
pub struct OperationProgress {
    pub kind: OperationKind,
    pub files_total: usize,
    pub files_done: usize,
    pub directories_done: usize,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub current_path: Option<String>,
    pub finished: bool,
    pub cancelled: bool,
    pub bytes_per_second: f64,
    pub eta_seconds: Option<f64>,
    pub current_file_bytes_done: u64,
    pub current_file_bytes_total: u64,
    pub failed_count: usize,
}

impl OperationProgress {
    pub fn percent(&self) -> f32 {
        if self.bytes_total == 0 {
            if self.finished {
                100.0
            } else {
                0.0
            }
        } else {
            ((self.bytes_done as f64 / self.bytes_total as f64) * 100.0).min(100.0) as f32
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OperationIssue {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct OperationReport {
    pub issues: Vec<OperationIssue>,
    pub files_skipped: usize,
    pub bytes_skipped: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FailurePolicy {
    #[default]
    StopOnFirstError,
    SkipFailedAndContinue,
}

#[derive(Debug, Clone, Default)]
pub struct OperationControl {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<OperationProgress>>,
    start_time: Arc<Mutex<Option<std::time::Instant>>>,
    report: Arc<Mutex<OperationReport>>,
}

impl OperationControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.with_progress(|progress| progress.cancelled = true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> OperationProgress {
        self.progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }

    pub fn report_snapshot(&self) -> OperationReport {
        self.report
            .lock()
            .map(|report| report.clone())
            .unwrap_or_default()
    }

    pub fn add_failed(&self) {
        self.with_progress(|progress| {
            progress.failed_count = progress.failed_count.saturating_add(1);
        });
    }

    pub fn set_current_file_bytes(&self, done: u64, total: u64) {
        self.with_progress(|progress| {
            progress.current_file_bytes_done = done;
            progress.current_file_bytes_total = total;
        });
    }

    pub fn add_issue(&self, path: &str, error: &str) {
        if let Ok(mut report) = self.report.lock() {
            report.issues.push(OperationIssue {
                path: path.to_string(),
                error: error.to_string(),
            });
        }
    }

    fn start(&self, kind: OperationKind, files_total: usize, bytes_total: u64) {
        self.cancelled.store(false, Ordering::Relaxed);
        let now = std::time::Instant::now();
        if let Ok(mut start) = self.start_time.lock() {
            *start = Some(now);
        }
        self.with_progress(|progress| {
            *progress = OperationProgress {
                kind,
                files_total,
                bytes_total,
                ..OperationProgress::default()
            };
        });
    }

    fn update_speed_and_eta(&self) {
        let elapsed = self
            .start_time
            .lock()
            .ok()
            .and_then(|start| (*start).map(|s| s.elapsed().as_secs_f64()))
            .unwrap_or(1.0);
        self.with_progress(|progress| {
            if elapsed > 0.0 && progress.bytes_done > 0 {
                progress.bytes_per_second = progress.bytes_done as f64 / elapsed;
                let remaining = progress.bytes_total.saturating_sub(progress.bytes_done);
                if progress.bytes_per_second > 0.0 {
                    progress.eta_seconds = Some(remaining as f64 / progress.bytes_per_second);
                }
            }
        });
    }

    fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(VaultError::OperationCancelled)
        } else {
            Ok(())
        }
    }

    fn set_current(&self, path: &Path) {
        let current_path = path.display().to_string();
        self.with_progress(|progress| progress.current_path = Some(current_path));
    }

    fn add_bytes(&self, bytes: u64) {
        self.with_progress(|progress| {
            progress.bytes_done = progress.bytes_done.saturating_add(bytes);
        });
        self.update_speed_and_eta();
    }

    fn file_done(&self) {
        self.with_progress(|progress| {
            progress.files_done = progress.files_done.saturating_add(1);
        });
    }

    fn directory_done(&self) {
        self.with_progress(|progress| {
            progress.directories_done = progress.directories_done.saturating_add(1);
        });
    }

    fn finish(&self) {
        self.with_progress(|progress| {
            progress.finished = true;
            progress.current_path = None;
            progress.current_file_bytes_done = 0;
            progress.current_file_bytes_total = 0;
        });
    }

    fn with_progress(&self, f: impl FnOnce(&mut OperationProgress)) {
        if let Ok(mut progress) = self.progress.lock() {
            f(&mut progress);
        }
    }
}

#[derive(Debug, Clone)]
pub struct VaultHandle {
    root: PathBuf,
    config: VaultConfig,
    vault_key: Zeroizing<[u8; KEY_LEN]>,
    index: VaultIndex,
    lock: Option<Arc<VaultLock>>,
}

#[derive(Debug)]
struct VaultLock {
    path: PathBuf,
    identity: PathBuf,
    file: fs::File,
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        if let Ok(mut locks) = active_vault_locks().lock() {
            locks.remove(&self.identity);
        }
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
    }
}

impl CustomVaultBackend {
    fn config_path(root: &Path) -> PathBuf {
        root.join(VAULT_CONFIG_FILE)
    }

    fn index_path(root: &Path) -> PathBuf {
        root.join(INDEX_FILE)
    }

    fn index_temp_path(root: &Path) -> PathBuf {
        root.join(INDEX_TEMP_FILE)
    }

    fn index_backup_path(root: &Path) -> PathBuf {
        root.join(INDEX_BACKUP_FILE)
    }

    fn blobs_path(root: &Path) -> PathBuf {
        root.join(BLOBS_DIR)
    }

    fn lock_path(root: &Path) -> PathBuf {
        root.join(LOCK_FILE)
    }

    fn read_config(root: &Path) -> Result<VaultConfig> {
        let path = Self::config_path(root);
        let data = fs::read_to_string(&path).map_err(|error| VaultError::io(&path, error))?;
        let config: VaultConfig = serde_json::from_str(&data)?;
        config.validate()?;
        Ok(config)
    }

    fn write_config(root: &Path, config: &VaultConfig) -> Result<()> {
        let path = Self::config_path(root);
        let data = serde_json::to_string_pretty(config)?;
        fs::write(&path, data).map_err(|error| VaultError::io(path, error))
    }

    pub fn check_health(
        path: impl AsRef<Path>,
        password: SecretString,
    ) -> Result<crate::health::VaultHealthReport> {
        let root = path.as_ref().to_path_buf();
        let config = match Self::read_config(&root) {
            Ok(config) => config,
            Err(VaultError::Io { .. })
            | Err(VaultError::Serialization(_))
            | Err(VaultError::InvalidFormat(_))
            | Err(VaultError::UnsupportedVersion { .. }) => {
                return Ok(crate::health::check_health_with_index_state(
                    &root, None, false,
                ));
            }
            Err(error) => return Err(error),
        };

        let vault_key = Self::unlock_vault_key(&config, &password)?;
        match read_index(&root, &vault_key) {
            Ok(index) => Ok(crate::health::check_health_with_index_state(
                &root,
                Some(&index),
                true,
            )),
            Err(VaultError::TamperedCiphertext)
            | Err(VaultError::InvalidFormat(_))
            | Err(VaultError::Serialization(_)) => Ok(
                crate::health::check_health_with_index_state(&root, None, false),
            ),
            Err(error) => Err(error),
        }
    }

    fn unlock_vault_key(config: &VaultConfig, password: &SecretString) -> Result<VaultKey> {
        let effective_version = config.effective_format_version();

        if effective_version >= 2 {
            let kek = CryptoProvider::derive_kek(password, &config.kdf)?;
            let wrapped = EncryptedPayload {
                nonce_b64: config.wrapped_key_nonce.clone(),
                ciphertext_b64: config.wrapped_vault_key.clone(),
            };
            CryptoProvider::unwrap_vault_key(&wrapped, &kek)
                .map_err(|_| VaultError::InvalidPassword)
        } else {
            CryptoProvider::derive_key(password, &config.kdf)
                .map_err(|_| VaultError::InvalidPassword)
        }
    }
}

impl VaultBackend for CustomVaultBackend {
    fn create(path: impl AsRef<Path>, password: SecretString) -> Result<VaultHandle> {
        let root = path.as_ref().to_path_buf();
        let config_path = Self::config_path(&root);
        if config_path.exists() {
            return Err(VaultError::VaultAlreadyExists { path: root });
        }
        if root.exists()
            && fs::read_dir(&root)
                .map_err(|error| VaultError::io(&root, error))?
                .next()
                .is_some()
        {
            return Err(VaultError::DirectoryNotEmpty { path: root });
        }
        fs::create_dir_all(&root).map_err(|error| VaultError::io(&root, error))?;
        fs::create_dir_all(Self::blobs_path(&root))
            .map_err(|error| VaultError::io(Self::blobs_path(&root), error))?;
        let lock = acquire_vault_lock(&root)?;
        cleanup_temp_files(&root)?;

        let mut config = VaultConfig::new();
        let vault_key = CryptoProvider::generate_vault_key();
        let kek = CryptoProvider::derive_kek(&password, &config.kdf)?;
        let wrapped = CryptoProvider::wrap_vault_key(&vault_key, &kek)?;
        config.wrapped_vault_key = wrapped.ciphertext_b64.clone();
        config.wrapped_key_nonce = wrapped.nonce_b64.clone();
        config.password_changed_at = Some(Utc::now());
        Self::write_config(&root, &config)?;

        let mut handle = VaultHandle {
            root,
            config,
            vault_key,
            index: VaultIndex::default(),
            lock: Some(lock),
        };
        handle.save_index()?;
        Ok(handle)
    }

    fn open(path: impl AsRef<Path>, password: SecretString) -> Result<VaultHandle> {
        let root = path.as_ref().to_path_buf();
        let config = Self::read_config(&root)?;
        let lock = acquire_vault_lock(&root)?;
        cleanup_temp_files(&root)?;

        let vault_key = Self::unlock_vault_key(&config, &password)?;

        let index = read_index(&root, &vault_key).map_err(|error| match error {
            VaultError::TamperedCiphertext | VaultError::InvalidFormat(_) => {
                VaultError::InvalidPassword
            }
            other => other,
        })?;
        validate_blob_references(&root, &index)?;

        Ok(VaultHandle {
            root,
            config,
            vault_key,
            index,
            lock: Some(lock),
        })
    }
}

impl VaultHandle {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    pub fn entries(&self) -> Vec<VaultEntry> {
        self.index.list().cloned().collect()
    }

    pub fn index(&self) -> &VaultIndex {
        &self.index
    }

    pub fn create_directory(&mut self, virtual_path: &VirtualPath) -> Result<ImportSummary> {
        if virtual_path == &VirtualPath::root() {
            return Err(VaultError::InvalidPath(
                "creating the vault root is not supported".to_string(),
            ));
        }
        if self.index.get(virtual_path).is_some() {
            return Err(VaultError::InvalidPath(format!(
                "entry already exists: {virtual_path}"
            )));
        }
        if let Some(parent) = virtual_path.parent() {
            if parent != VirtualPath::root() {
                match self.index.get(&parent) {
                    Some(entry) if entry.kind == EntryKind::Directory => {}
                    _ => {
                        return Err(VaultError::InvalidPath(format!(
                            "parent directory does not exist: {parent}"
                        )));
                    }
                }
            }
        }

        self.index
            .insert(VaultEntry::directory(virtual_path.clone()));
        self.save_index()?;
        Ok(ImportSummary {
            files: 0,
            directories: 1,
            bytes: 0,
        })
    }

    pub fn rename_entry(
        &mut self,
        virtual_path: &VirtualPath,
        new_name: &str,
    ) -> Result<ImportSummary> {
        if virtual_path == &VirtualPath::root() {
            return Err(VaultError::InvalidPath(
                "renaming the vault root is not supported".to_string(),
            ));
        }
        if new_name.trim().is_empty() || new_name.contains('/') || new_name.contains('\\') {
            return Err(VaultError::InvalidPath(
                "new name must be a single file or folder name".to_string(),
            ));
        }

        let Some(entry) = self.index.get(virtual_path).cloned() else {
            return Err(VaultError::InvalidPath(format!(
                "entry not found: {virtual_path}"
            )));
        };
        let parent = virtual_path.parent().unwrap_or_else(VirtualPath::root);
        let new_path = parent.join(new_name)?;
        if self.index.get(&new_path).is_some() {
            return Err(VaultError::InvalidPath(format!(
                "entry already exists: {new_path}"
            )));
        }

        let old_prefix = format!("{}/", entry.virtual_path.as_str().trim_end_matches('/'));
        let mut moved = Vec::new();
        let mut summary = ImportSummary::default();

        for candidate in self.index.list() {
            if candidate.virtual_path == entry.virtual_path
                || (entry.kind == EntryKind::Directory
                    && candidate.virtual_path.as_str().starts_with(&old_prefix))
            {
                let mut updated = candidate.clone();
                updated.virtual_path = if candidate.virtual_path == entry.virtual_path {
                    new_path.clone()
                } else {
                    let suffix = candidate
                        .virtual_path
                        .as_str()
                        .strip_prefix(&old_prefix)
                        .unwrap_or_default();
                    new_path.join(suffix)?
                };
                if updated.kind == EntryKind::File && candidate.virtual_path == entry.virtual_path {
                    let file_name = updated.virtual_path.file_name().unwrap_or_default();
                    updated.encrypted_name = Some(CryptoProvider::encrypt(
                        &self.vault_key,
                        file_name.as_bytes(),
                    )?);
                }
                match updated.kind {
                    EntryKind::Directory => summary.directories += 1,
                    EntryKind::File => {
                        summary.files += 1;
                        summary.bytes = summary.bytes.saturating_add(updated.size);
                    }
                }
                moved.push((candidate.virtual_path.clone(), updated));
            }
        }

        let original_index = self.index.clone();
        for (old_path, _) in &moved {
            self.index.entries.remove(old_path);
        }
        for (_, updated) in moved {
            self.index.insert(updated);
        }

        if let Err(error) = self.save_index() {
            self.index = original_index;
            return Err(error);
        }

        Ok(summary)
    }

    pub fn move_entry(
        &mut self,
        virtual_path: &VirtualPath,
        dest_dir: &VirtualPath,
    ) -> Result<ImportSummary> {
        if virtual_path == &VirtualPath::root() {
            return Err(VaultError::InvalidPath(
                "moving the vault root is not supported".to_string(),
            ));
        }

        let Some(entry) = self.index.get(virtual_path).cloned() else {
            return Err(VaultError::InvalidPath(format!(
                "entry not found: {virtual_path}"
            )));
        };

        // Dest must be a directory in the index
        if dest_dir != &VirtualPath::root() {
            match self.index.get(dest_dir) {
                Some(e) if e.kind == EntryKind::Directory => {}
                Some(_) => {
                    return Err(VaultError::InvalidPath(format!(
                        "destination is not a directory: {dest_dir}"
                    )));
                }
                None => {
                    return Err(VaultError::InvalidPath(format!(
                        "destination folder not found: {dest_dir}"
                    )));
                }
            }
        }

        let source_parent = virtual_path.parent().unwrap_or_else(VirtualPath::root);
        if dest_dir == &source_parent {
            return Err(VaultError::InvalidPath(
                "entry is already in this folder".to_string(),
            ));
        }

        // Prevent moving a directory into itself or its descendants
        if entry.kind == EntryKind::Directory {
            let source_prefix = format!("{}/", virtual_path.as_str().trim_end_matches('/'));
            let dest_str = dest_dir.as_str();
            if dest_str == virtual_path.as_str() || dest_str.starts_with(&source_prefix) {
                return Err(VaultError::InvalidPath(
                    "cannot move a folder into itself or its subfolder".to_string(),
                ));
            }
        }

        let file_name = virtual_path.file_name().unwrap_or_default();
        let new_path = dest_dir.join(file_name)?;

        if self.index.get(&new_path).is_some() {
            return Err(VaultError::InvalidPath(format!(
                "entry already exists at destination: {new_path}"
            )));
        }

        let old_prefix = format!("{}/", entry.virtual_path.as_str().trim_end_matches('/'));
        let new_prefix = format!("{}/", new_path.as_str().trim_end_matches('/'));
        let mut moved = Vec::new();
        let mut summary = ImportSummary::default();

        for candidate in self.index.list() {
            if candidate.virtual_path == entry.virtual_path
                || (entry.kind == EntryKind::Directory
                    && candidate.virtual_path.as_str().starts_with(&old_prefix))
            {
                let mut updated = candidate.clone();
                updated.virtual_path = if candidate.virtual_path == entry.virtual_path {
                    new_path.clone()
                } else {
                    let suffix = candidate
                        .virtual_path
                        .as_str()
                        .strip_prefix(&old_prefix)
                        .unwrap_or_default();
                    // Build the new path by appending suffix to new_prefix
                    VirtualPath::new(format!("{new_prefix}{suffix}"))?
                };
                if updated.kind == EntryKind::File && candidate.virtual_path == entry.virtual_path {
                    let fname = updated.virtual_path.file_name().unwrap_or_default();
                    updated.encrypted_name =
                        Some(CryptoProvider::encrypt(&self.vault_key, fname.as_bytes())?);
                }
                match updated.kind {
                    EntryKind::Directory => summary.directories += 1,
                    EntryKind::File => {
                        summary.files += 1;
                        summary.bytes = summary.bytes.saturating_add(updated.size);
                    }
                }
                moved.push((candidate.virtual_path.clone(), updated));
            }
        }

        let original_index = self.index.clone();
        for (old_path, _) in &moved {
            self.index.entries.remove(old_path);
        }
        for (_, updated) in moved {
            self.index.insert(updated);
        }

        if let Err(error) = self.save_index() {
            self.index = original_index;
            return Err(error);
        }

        Ok(summary)
    }

    pub fn entry_summary(&self, virtual_path: &VirtualPath) -> Result<ImportSummary> {
        let Some(entry) = self.index.get(virtual_path) else {
            return Err(VaultError::InvalidPath(format!(
                "entry not found: {virtual_path}"
            )));
        };
        let prefix = format!("{}/", entry.virtual_path.as_str().trim_end_matches('/'));
        let mut summary = ImportSummary::default();

        for candidate in self.index.list() {
            if candidate.virtual_path == entry.virtual_path
                || (entry.kind == EntryKind::Directory
                    && candidate.virtual_path.as_str().starts_with(&prefix))
            {
                match candidate.kind {
                    EntryKind::Directory => summary.directories += 1,
                    EntryKind::File => {
                        summary.files += 1;
                        summary.bytes = summary.bytes.saturating_add(candidate.size);
                    }
                }
            }
        }

        Ok(summary)
    }

    pub fn cleanup_unreferenced_blobs(&self) -> Result<usize> {
        cleanup_unreferenced_blobs(&self.root, &self.index)
    }

    pub fn validate_blobs(&self) -> Result<()> {
        validate_blob_references(&self.root, &self.index)
    }

    pub fn check_health(&self) -> crate::health::VaultHealthReport {
        match read_index(&self.root, &self.vault_key) {
            Ok(_) => {
                crate::health::check_health_with_index_state(&self.root, Some(&self.index), true)
            }
            Err(_) => crate::health::check_health_with_index_state(&self.root, None, false),
        }
    }

    pub fn cleanup_orphan_blobs(&self) -> Result<crate::health::CleanupSummary> {
        crate::health::cleanup_orphan_blobs_detailed(&self.root, &self.index)
    }

    pub fn change_password(
        &mut self,
        old_password: &SecretString,
        new_password: &SecretString,
    ) -> Result<()> {
        let effective_version = self.config.effective_format_version();
        let mut new_config = self.config.clone();

        if effective_version >= 2 {
            let kek = CryptoProvider::derive_kek(old_password, &self.config.kdf)?;
            let wrapped = EncryptedPayload {
                nonce_b64: self.config.wrapped_key_nonce.clone(),
                ciphertext_b64: self.config.wrapped_vault_key.clone(),
            };
            CryptoProvider::unwrap_vault_key(&wrapped, &kek)
                .map_err(|_| VaultError::InvalidPassword)?;
        } else {
            let derived = CryptoProvider::derive_key(old_password, &self.config.kdf)
                .map_err(|_| VaultError::InvalidPassword)?;
            if derived.as_slice() != self.vault_key.as_slice() {
                return Err(VaultError::InvalidPassword);
            }
            new_config.format_version = CURRENT_FORMAT_VERSION;
            let kek = CryptoProvider::derive_kek(old_password, &self.config.kdf)?;
            let wrapped = CryptoProvider::wrap_vault_key(&self.vault_key, &kek)?;
            new_config.wrapped_vault_key = wrapped.ciphertext_b64.clone();
            new_config.wrapped_key_nonce = wrapped.nonce_b64.clone();
        }

        new_config.kdf = KdfConfig::new_interactive();
        let mut kdf_salt_bytes = [0_u8; 16];
        OsRng.fill_bytes(&mut kdf_salt_bytes);
        new_config.kdf_salt =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, kdf_salt_bytes);

        let new_kek = CryptoProvider::derive_kek(new_password, &new_config.kdf)?;
        let new_wrapped = CryptoProvider::wrap_vault_key(&self.vault_key, &new_kek)?;
        new_config.wrapped_vault_key = new_wrapped.ciphertext_b64;
        new_config.wrapped_key_nonce = new_wrapped.nonce_b64;
        new_config.password_changed_at = Some(Utc::now());

        self.write_config_atomic(&new_config)?;
        self.config = new_config;
        Ok(())
    }

    fn write_config_atomic(&self, config: &VaultConfig) -> Result<()> {
        let path = CustomVaultBackend::config_path(&self.root);
        let path_str = path.display().to_string();
        let temp_path = PathBuf::from(format!("{path_str}.tmp"));
        let bak_path = PathBuf::from(format!("{path_str}.bak"));

        let data = serde_json::to_string_pretty(config)?;
        fs::write(&temp_path, &data).map_err(|error| VaultError::io(&temp_path, error))?;

        let verify =
            fs::read_to_string(&temp_path).map_err(|error| VaultError::io(&temp_path, error))?;
        let _: VaultConfig = serde_json::from_str(&verify)?;

        if !path.exists() {
            fs::rename(&temp_path, &path).map_err(|error| VaultError::io(&path, error))?;
            return Ok(());
        }

        let _ = fs::remove_file(&bak_path);
        fs::rename(&path, &bak_path).map_err(|error| VaultError::io(&bak_path, error))?;

        if let Err(error) = fs::rename(&temp_path, &path) {
            let _ = fs::rename(&bak_path, &path);
            return Err(VaultError::io(&path, error));
        }

        Ok(())
    }

    pub fn import_path(&mut self, source: impl AsRef<Path>) -> Result<ImportSummary> {
        self.import_path_with_control(source, &OperationControl::new())
    }

    pub fn import_path_with_control(
        &mut self,
        source: impl AsRef<Path>,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        self.import_path_with_options_and_control(source, ImportOptions::default(), control)
    }

    pub fn import_path_with_options(
        &mut self,
        source: impl AsRef<Path>,
        options: ImportOptions,
    ) -> Result<ImportSummary> {
        self.import_path_with_options_and_control(source, options, &OperationControl::new())
    }

    pub fn import_path_with_options_and_control(
        &mut self,
        source: impl AsRef<Path>,
        options: ImportOptions,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        let source = source.as_ref();
        if path_is_same_or_inside(source, &self.root)? {
            return Err(VaultError::ImportSourceInsideVault {
                path: source.to_path_buf(),
            });
        }
        cleanup_temp_files(&self.root)?;
        let plan = ImportPlan::new(source)?;
        ensure_available_space(&self.root, estimated_encrypted_size(plan.bytes))?;
        control.start(OperationKind::Import, plan.files, plan.bytes);

        let mut summary = ImportSummary::default();
        let original_index = self.index.clone();
        let mut new_blob_ids = Vec::new();

        let mut skipped_files = 0_usize;
        let mut skipped_bytes = 0_u64;

        for path in plan.paths {
            let result = self.import_single_path(&path, &plan.base, &mut summary, control, options);
            match result {
                Ok(Some(blob_id)) => new_blob_ids.push(blob_id),
                Ok(None) => {}
                Err(error) => {
                    if options.failure_policy == FailurePolicy::SkipFailedAndContinue {
                        control.add_failed();
                        let error_msg = error.to_string();
                        control.add_issue(&path.display().to_string(), &error_msg);
                        if let Ok(metadata) = fs::metadata(&path) {
                            skipped_bytes = skipped_bytes.saturating_add(metadata.len());
                        }
                        skipped_files += 1;
                        continue;
                    }
                    self.index = original_index;
                    for blob_id in new_blob_ids {
                        let _ = fs::remove_file(self.blob_path(&blob_id));
                    }
                    return Err(error);
                }
            }
        }

        if skipped_files > 0 {
            if let Ok(mut report) = control.report.lock() {
                report.files_skipped = skipped_files;
                report.bytes_skipped = skipped_bytes;
            }
        }

        self.save_index()?;
        self.cleanup_unreferenced_blobs()?;
        control.finish();
        Ok(summary)
    }

    pub fn export_all(&self, destination: impl AsRef<Path>) -> Result<ImportSummary> {
        self.export_all_with_control(destination, &OperationControl::new())
    }

    pub fn export_all_with_control(
        &self,
        destination: impl AsRef<Path>,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        self.export_all_with_policy_and_control(
            destination,
            FailurePolicy::StopOnFirstError,
            control,
        )
    }

    pub fn export_all_with_policy_and_control(
        &self,
        destination: impl AsRef<Path>,
        failure_policy: FailurePolicy,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        let destination = destination.as_ref();
        if path_is_same_or_inside(destination, &self.root)? {
            return Err(VaultError::ExportDestinationInsideVault {
                destination: destination.to_path_buf(),
            });
        }
        fs::create_dir_all(destination).map_err(|error| VaultError::io(destination, error))?;
        cleanup_temp_files(destination)?;

        let total_bytes = self
            .index
            .list()
            .filter(|entry| entry.kind == EntryKind::File)
            .map(|entry| entry.size)
            .sum::<u64>();
        let total_files = self
            .index
            .list()
            .filter(|entry| entry.kind == EntryKind::File)
            .count();
        ensure_available_space(destination, total_bytes)?;
        control.start(OperationKind::Export, total_files, total_bytes);

        let mut summary = ImportSummary::default();
        let mut created_files = Vec::new();
        let mut skipped_files = 0_usize;
        let mut skipped_bytes = 0_u64;
        for entry in self.index.list() {
            control.check_cancelled()?;
            let output_path = destination.join(entry.virtual_path.to_safe_os_path());
            match entry.kind {
                EntryKind::Directory => {
                    if let Err(error) = fs::create_dir_all(&output_path)
                        .map_err(|error| VaultError::io(&output_path, error))
                    {
                        if failure_policy == FailurePolicy::SkipFailedAndContinue {
                            control.add_failed();
                            control.add_issue(&entry.virtual_path.to_string(), &error.to_string());
                            continue;
                        }
                        return Err(error);
                    }
                    summary.directories += 1;
                }
                EntryKind::File => {
                    control.set_current(&output_path);
                    if let Some(parent) = output_path.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|error| VaultError::io(parent, error))?;
                    }
                    let blob_id = entry.blob_id.as_ref().ok_or_else(|| {
                        VaultError::InvalidFormat(format!(
                            "file entry {} has no blob id",
                            entry.virtual_path
                        ))
                    })?;
                    if let Err(error) =
                        self.decrypt_blob(blob_id, &output_path, entry.size, control)
                    {
                        let _ = fs::remove_file(&output_path);
                        if failure_policy == FailurePolicy::SkipFailedAndContinue {
                            control.add_failed();
                            control.add_issue(&entry.virtual_path.to_string(), &error.to_string());
                            skipped_files = skipped_files.saturating_add(1);
                            skipped_bytes = skipped_bytes.saturating_add(entry.size);
                            continue;
                        }
                        for file in created_files {
                            let _ = fs::remove_file(file);
                        }
                        return Err(error);
                    }
                    created_files.push(output_path);
                    summary.files += 1;
                    summary.bytes += entry.size;
                    control.file_done();
                }
            }
        }

        if skipped_files > 0 {
            if let Ok(mut report) = control.report.lock() {
                report.files_skipped = report.files_skipped.saturating_add(skipped_files);
                report.bytes_skipped = report.bytes_skipped.saturating_add(skipped_bytes);
            }
        }

        control.finish();
        Ok(summary)
    }

    pub fn export_entry(
        &self,
        virtual_path: &VirtualPath,
        destination: impl AsRef<Path>,
    ) -> Result<ImportSummary> {
        self.export_entry_with_control(virtual_path, destination, &OperationControl::new())
    }

    pub fn export_entry_with_control(
        &self,
        virtual_path: &VirtualPath,
        destination: impl AsRef<Path>,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        self.export_entry_with_policy_and_control(
            virtual_path,
            destination,
            FailurePolicy::StopOnFirstError,
            control,
        )
    }

    pub fn export_entry_with_policy_and_control(
        &self,
        virtual_path: &VirtualPath,
        destination: impl AsRef<Path>,
        failure_policy: FailurePolicy,
        control: &OperationControl,
    ) -> Result<ImportSummary> {
        let destination = destination.as_ref();
        if path_is_same_or_inside(destination, &self.root)? {
            return Err(VaultError::ExportDestinationInsideVault {
                destination: destination.to_path_buf(),
            });
        }
        let Some(entry) = self.index.get(virtual_path) else {
            return Err(VaultError::InvalidPath(format!(
                "entry not found: {virtual_path}"
            )));
        };

        match entry.kind {
            EntryKind::Directory => {
                let prefix = format!("{}/", entry.virtual_path.as_str().trim_end_matches('/'));
                let mut filtered = VaultIndex::default();
                for candidate in self.index.list() {
                    if candidate.virtual_path == entry.virtual_path
                        || candidate.virtual_path.as_str().starts_with(&prefix)
                    {
                        filtered.insert(candidate.clone());
                    }
                }
                let tmp = VaultHandle {
                    root: self.root.clone(),
                    config: self.config.clone(),
                    vault_key: Zeroizing::new(*self.vault_key),
                    index: filtered,
                    lock: self.lock.clone(),
                };
                tmp.export_all_with_policy_and_control(destination, failure_policy, control)
            }
            EntryKind::File => {
                fs::create_dir_all(destination)
                    .map_err(|error| VaultError::io(destination, error))?;
                ensure_available_space(destination, entry.size)?;
                control.start(OperationKind::Export, 1, entry.size);
                let output_path = destination.join(entry.virtual_path.to_safe_os_path());
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent).map_err(|error| VaultError::io(parent, error))?;
                }
                let blob_id = entry.blob_id.as_ref().ok_or_else(|| {
                    VaultError::InvalidFormat(format!(
                        "file entry {} has no blob id",
                        entry.virtual_path
                    ))
                })?;
                if let Err(error) = self.decrypt_blob(blob_id, &output_path, entry.size, control) {
                    let _ = fs::remove_file(&output_path);
                    if failure_policy == FailurePolicy::SkipFailedAndContinue {
                        control.add_failed();
                        control.add_issue(&entry.virtual_path.to_string(), &error.to_string());
                        if let Ok(mut report) = control.report.lock() {
                            report.files_skipped = report.files_skipped.saturating_add(1);
                            report.bytes_skipped = report.bytes_skipped.saturating_add(entry.size);
                        }
                        control.finish();
                        return Ok(ImportSummary::default());
                    }
                    return Err(error);
                }
                control.file_done();
                control.finish();
                Ok(ImportSummary {
                    files: 1,
                    directories: 0,
                    bytes: entry.size,
                })
            }
        }
    }

    pub fn delete_entry(&mut self, virtual_path: &VirtualPath) -> Result<ImportSummary> {
        if virtual_path == &VirtualPath::root() {
            return Err(VaultError::InvalidPath(
                "deleting the vault root is not supported".to_string(),
            ));
        }

        let Some(entry) = self.index.get(virtual_path) else {
            return Err(VaultError::InvalidPath(format!(
                "entry not found: {virtual_path}"
            )));
        };

        let prefix = format!("{}/", entry.virtual_path.as_str().trim_end_matches('/'));
        let mut paths_to_remove = Vec::new();
        let mut blob_ids = Vec::new();
        let mut summary = ImportSummary::default();

        for candidate in self.index.list() {
            if candidate.virtual_path == entry.virtual_path
                || (entry.kind == EntryKind::Directory
                    && candidate.virtual_path.as_str().starts_with(&prefix))
            {
                paths_to_remove.push(candidate.virtual_path.clone());
                match candidate.kind {
                    EntryKind::Directory => summary.directories += 1,
                    EntryKind::File => {
                        summary.files += 1;
                        summary.bytes = summary.bytes.saturating_add(candidate.size);
                        if let Some(blob_id) = &candidate.blob_id {
                            blob_ids.push(blob_id.clone());
                        }
                    }
                }
            }
        }

        let original_index = self.index.clone();
        for path in paths_to_remove {
            self.index.entries.remove(&path);
        }

        if let Err(error) = self.save_index() {
            self.index = original_index;
            return Err(error);
        }

        for blob_id in blob_ids {
            let blob_path = self.blob_path(&blob_id);
            if let Err(error) = fs::remove_file(&blob_path) {
                if error.kind() != ErrorKind::NotFound {
                    return Err(VaultError::io(blob_path, error));
                }
            }
        }

        Ok(summary)
    }

    pub fn save_index(&mut self) -> Result<()> {
        let path = CustomVaultBackend::index_path(&self.root);
        let temp_path = CustomVaultBackend::index_temp_path(&self.root);
        let backup_path = CustomVaultBackend::index_backup_path(&self.root);
        let plaintext = serde_json::to_vec(&self.index)?;
        let encrypted = CryptoProvider::encrypt(&self.vault_key, &plaintext)?;
        let data = serde_json::to_vec_pretty(&encrypted)?;

        {
            let mut file =
                fs::File::create(&temp_path).map_err(|error| VaultError::io(&temp_path, error))?;
            file.write_all(&data)
                .map_err(|error| VaultError::io(&temp_path, error))?;
            file.sync_all()
                .map_err(|error| VaultError::io(&temp_path, error))?;
        }

        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|error| VaultError::io(&backup_path, error))?;
        }
        if path.exists() {
            fs::rename(&path, &backup_path).map_err(|error| VaultError::io(&backup_path, error))?;
        }

        if let Err(error) = fs::rename(&temp_path, &path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, &path);
            }
            return Err(VaultError::io(&path, error));
        }

        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|error| VaultError::io(&backup_path, error))?;
        }

        Ok(())
    }

    fn import_single_path(
        &mut self,
        path: &Path,
        base: &Path,
        summary: &mut ImportSummary,
        control: &OperationControl,
        options: ImportOptions,
    ) -> Result<Option<String>> {
        control.check_cancelled()?;
        control.set_current(path);
        let relative = path
            .strip_prefix(base)
            .map_err(|_| VaultError::PathOutsideRoot {
                path: path.to_path_buf(),
            })?;
        let desired_virtual_path = VirtualPath::from_relative_path(relative)?;
        let metadata = fs::metadata(path).map_err(|error| VaultError::io(path, error))?;

        if metadata.is_dir() {
            let Some(virtual_path) = resolve_import_path(
                &self.index,
                &desired_virtual_path,
                EntryKind::Directory,
                options.conflict_policy,
            )?
            else {
                return Ok(None);
            };
            if self.index.get(&virtual_path).is_some() {
                return Ok(None);
            }
            self.index.insert(VaultEntry::directory(virtual_path));
            summary.directories += 1;
            control.directory_done();
            return Ok(None);
        }

        if metadata.is_file() {
            let Some(virtual_path) = resolve_import_path(
                &self.index,
                &desired_virtual_path,
                EntryKind::File,
                options.conflict_policy,
            )?
            else {
                control.file_done();
                return Ok(None);
            };
            if let Some(parent) = virtual_path.parent() {
                if parent != VirtualPath::root() && self.index.get(&parent).is_none() {
                    self.index.insert(VaultEntry::directory(parent));
                }
            }

            let file_name = virtual_path.file_name().unwrap_or_default();
            let encrypted_name = CryptoProvider::encrypt(&self.vault_key, file_name.as_bytes())?;
            let blob_id = self.encrypt_file_to_blob(path, control)?;
            let modified_at = metadata.modified().ok().map(system_time_to_utc);
            let size = metadata.len();
            self.index.insert(VaultEntry::file(
                virtual_path,
                size,
                modified_at,
                blob_id.clone(),
                encrypted_name,
            ));
            summary.files += 1;
            summary.bytes += size;
            control.file_done();
            return Ok(Some(blob_id));
        }

        Ok(None)
    }

    fn encrypt_file_to_blob(&self, source: &Path, control: &OperationControl) -> Result<String> {
        let mut file =
            BufReader::new(fs::File::open(source).map_err(|error| VaultError::io(source, error))?);
        let mut blob_salt = [0_u8; 16];
        OsRng.fill_bytes(&mut blob_salt);

        let mut hasher = Sha256::new();
        hasher.update(blob_salt);
        hasher.update(source.to_string_lossy().as_bytes());
        hasher.update(
            Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
                .to_le_bytes(),
        );
        let blob_id = hex::encode(hasher.finalize());
        let blob_path = self.blob_path(&blob_id);
        let temp_blob_path = blob_path.with_extension(TEMP_EXTENSION);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).map_err(|error| VaultError::io(parent, error))?;
        }

        let result = (|| {
            let mut output = BufWriter::new(
                fs::File::create(&temp_blob_path)
                    .map_err(|error| VaultError::io(&temp_blob_path, error))?,
            );
            write_binary_blob_header(&mut output)?;

            let mut chunk_index = 0_u64;
            let mut bytes_written = 0_u64;
            let source_len = fs::metadata(source)
                .map_err(|error| VaultError::io(source, error))?
                .len();
            let aad_prefix = blob_aad_prefix(&blob_id);

            let mut buffer = vec![0_u8; CHUNK_SIZE];
            loop {
                control.check_cancelled()?;
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| VaultError::io(source, error))?;
                if read == 0 {
                    break;
                }

                let aad = blob_chunk_aad(&aad_prefix, chunk_index);
                let (nonce, ciphertext) =
                    CryptoProvider::encrypt_raw_with_aad(&self.vault_key, &buffer[..read], &aad)?;
                output
                    .write_all(&nonce)
                    .map_err(|error| VaultError::io(&temp_blob_path, error))?;
                output
                    .write_all(&(ciphertext.len() as u64).to_le_bytes())
                    .map_err(|error| VaultError::io(&temp_blob_path, error))?;
                output
                    .write_all(&ciphertext)
                    .map_err(|error| VaultError::io(&temp_blob_path, error))?;

                chunk_index = chunk_index.saturating_add(1);
                bytes_written = bytes_written.saturating_add(read as u64);
                control.add_bytes(read as u64);
                control.set_current_file_bytes(bytes_written, source_len);
            }

            if bytes_written != source_len {
                return Err(VaultError::InvalidFormat(format!(
                    "source changed while importing: {}",
                    source.display()
                )));
            }

            output
                .flush()
                .map_err(|error| VaultError::io(&temp_blob_path, error))?;
            drop(output);
            fs::rename(&temp_blob_path, &blob_path)
                .map_err(|error| VaultError::io(&blob_path, error))?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_blob_path);
        }
        result?;
        Ok(blob_id)
    }

    fn decrypt_blob(
        &self,
        blob_id: &str,
        destination: &Path,
        expected_size: u64,
        control: &OperationControl,
    ) -> Result<()> {
        let blob_path = self.blob_path(blob_id);
        let mut input = BufReader::new(
            fs::File::open(&blob_path).map_err(|error| VaultError::io(&blob_path, error))?,
        );
        let mut magic = [0_u8; 8];
        input
            .read_exact(&mut magic)
            .map_err(|error| VaultError::io(&blob_path, error))?;

        if &magic == BINARY_BLOB_MAGIC {
            return self.decrypt_binary_blob(blob_id, input, destination, expected_size, control);
        }

        self.decrypt_legacy_json_blob(blob_path, destination, expected_size)
    }

    fn decrypt_binary_blob(
        &self,
        blob_id: &str,
        mut input: BufReader<fs::File>,
        destination: &Path,
        expected_size: u64,
        control: &OperationControl,
    ) -> Result<()> {
        let version = read_u16(&mut input, destination)?;
        if version != BINARY_BLOB_VERSION {
            return Err(VaultError::UnsupportedVersion {
                found: u32::from(version),
            });
        }

        let chunk_size = read_u32(&mut input, destination)?;
        if chunk_size == 0 || chunk_size as usize > CHUNK_SIZE {
            return Err(VaultError::InvalidFormat(format!(
                "invalid blob chunk size: {chunk_size}"
            )));
        }

        let temp_destination = destination.with_extension(TEMP_EXTENSION);
        let result = (|| {
            let mut output = BufWriter::new(
                fs::File::create(&temp_destination)
                    .map_err(|error| VaultError::io(&temp_destination, error))?,
            );
            let aad_prefix = blob_aad_prefix(blob_id);
            let mut chunk_index = 0_u64;
            let mut output_bytes = 0_u64;

            loop {
                control.check_cancelled()?;
                let mut nonce = [0_u8; XCHACHA_NONCE_LEN];
                match input.read_exact(&mut nonce) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
                    Err(error) => return Err(VaultError::io(destination, error)),
                }

                let ciphertext_len = read_u64(&mut input, destination)?;
                if ciphertext_len > (CHUNK_SIZE as u64 + 64) {
                    return Err(VaultError::InvalidFormat(format!(
                        "invalid ciphertext chunk length: {ciphertext_len}"
                    )));
                }

                let mut ciphertext = vec![0_u8; ciphertext_len as usize];
                input
                    .read_exact(&mut ciphertext)
                    .map_err(|error| VaultError::io(destination, error))?;
                let aad = blob_chunk_aad(&aad_prefix, chunk_index);
                let plaintext = CryptoProvider::decrypt_raw_with_aad(
                    &self.vault_key,
                    &nonce,
                    &ciphertext,
                    &aad,
                )?;
                output
                    .write_all(&plaintext)
                    .map_err(|error| VaultError::io(&temp_destination, error))?;
                let plaintext_len = plaintext.len() as u64;
                output_bytes = output_bytes.saturating_add(plaintext_len);
                control.add_bytes(plaintext_len);
                control.set_current_file_bytes(output_bytes, expected_size);
                chunk_index = chunk_index.saturating_add(1);
            }

            if output_bytes != expected_size {
                return Err(VaultError::TamperedCiphertext);
            }

            output
                .flush()
                .map_err(|error| VaultError::io(&temp_destination, error))?;
            drop(output);
            fs::rename(&temp_destination, destination)
                .map_err(|error| VaultError::io(destination, error))?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_destination);
        }
        result
    }

    fn decrypt_legacy_json_blob(
        &self,
        blob_path: PathBuf,
        destination: &Path,
        expected_size: u64,
    ) -> Result<()> {
        let data = fs::read(&blob_path).map_err(|error| VaultError::io(&blob_path, error))?;
        let blob: EncryptedBlob = serde_json::from_slice(&data)?;
        if blob.version != 1 {
            return Err(VaultError::UnsupportedVersion {
                found: blob.version,
            });
        }

        let mut output =
            fs::File::create(destination).map_err(|error| VaultError::io(destination, error))?;
        let mut output_bytes = 0_u64;
        for chunk in blob.chunks {
            let plaintext = CryptoProvider::decrypt(&self.vault_key, &chunk)?;
            output
                .write_all(plaintext.as_slice())
                .map_err(|error| VaultError::io(destination, error))?;
            output_bytes = output_bytes.saturating_add(plaintext.len() as u64);
        }
        if output_bytes != expected_size {
            return Err(VaultError::TamperedCiphertext);
        }
        Ok(())
    }

    fn blob_path(&self, blob_id: &str) -> PathBuf {
        let prefix = blob_id.get(0..2).unwrap_or("00");
        CustomVaultBackend::blobs_path(&self.root)
            .join(prefix)
            .join(format!("{blob_id}.bin"))
    }
}

fn read_index(root: &Path, key: &[u8; KEY_LEN]) -> Result<VaultIndex> {
    let path = CustomVaultBackend::index_path(root);
    let backup_path = CustomVaultBackend::index_backup_path(root);
    if !path.exists() {
        if backup_path.exists() {
            return read_index_file(&backup_path, key);
        }
        return Ok(VaultIndex::default());
    }

    read_index_file(&path, key)
}

fn read_index_file(path: &Path, key: &[u8; KEY_LEN]) -> Result<VaultIndex> {
    let data = fs::read(&path).map_err(|error| VaultError::io(&path, error))?;
    let encrypted: EncryptedPayload = serde_json::from_slice(&data)?;
    let plaintext = CryptoProvider::decrypt(key, &encrypted)?;
    serde_json::from_slice(plaintext.as_slice()).map_err(VaultError::from)
}

fn acquire_vault_lock(root: &Path) -> Result<Arc<VaultLock>> {
    let path = CustomVaultBackend::lock_path(root);
    let identity = absolute_lexical(&path)?;
    let mut active_locks = active_vault_locks()
        .lock()
        .map_err(|_| VaultError::InvalidFormat("vault lock registry is poisoned".to_string()))?;
    if active_locks.contains(&identity) {
        return Err(VaultError::VaultLocked { path });
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| VaultError::io(&path, error))?;

    if let Err(error) = file.try_lock_exclusive() {
        if error.kind() == ErrorKind::WouldBlock {
            return Err(VaultError::VaultLocked { path });
        }
        return Err(VaultError::io(&path, error));
    }

    let content = format!(
        "pid={}\ncreated_at={}\n",
        std::process::id(),
        Utc::now().to_rfc3339()
    );
    file.set_len(0)
        .map_err(|error| VaultError::io(&path, error))?;
    file.write_all(content.as_bytes())
        .map_err(|error| VaultError::io(&path, error))?;

    active_locks.insert(identity.clone());
    Ok(Arc::new(VaultLock {
        path,
        identity,
        file,
    }))
}

fn active_vault_locks() -> &'static Mutex<HashSet<PathBuf>> {
    ACTIVE_VAULT_LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn validate_blob_references(root: &Path, index: &VaultIndex) -> Result<()> {
    for entry in index.list().filter(|entry| entry.kind == EntryKind::File) {
        let Some(blob_id) = &entry.blob_id else {
            return Err(VaultError::InvalidFormat(format!(
                "file entry {} has no blob id",
                entry.virtual_path
            )));
        };
        let prefix = blob_id.get(0..2).unwrap_or("00");
        let blob_path = CustomVaultBackend::blobs_path(root)
            .join(prefix)
            .join(format!("{blob_id}.bin"));
        if !blob_path.exists() {
            return Err(VaultError::MissingBlob {
                virtual_path: entry.virtual_path.to_string(),
                blob_id: blob_id.clone(),
            });
        }
    }

    Ok(())
}

fn system_time_to_utc(time: SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(time)
}

fn path_is_same_or_inside(path: &Path, root: &Path) -> Result<bool> {
    let path = absolute_lexical(path)?;
    let root = absolute_lexical(root)?;
    Ok(path == root || path.starts_with(root))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| VaultError::io(PathBuf::from("."), error))?
            .join(path)
    };

    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }

    Ok(normalized)
}

struct ImportPlan {
    base: PathBuf,
    paths: Vec<PathBuf>,
    files: usize,
    bytes: u64,
}

impl ImportPlan {
    fn new(source: &Path) -> Result<Self> {
        let metadata = fs::metadata(source).map_err(|error| VaultError::io(source, error))?;
        let base = if metadata.is_dir() {
            source.to_path_buf()
        } else {
            source
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf()
        };

        let mut paths = Vec::new();
        let mut files = 0_usize;
        let mut bytes = 0_u64;

        if metadata.is_dir() {
            for entry in WalkDir::new(source) {
                let entry = entry?;
                let path = entry.path();
                if path == source {
                    continue;
                }
                let metadata = fs::metadata(path).map_err(|error| VaultError::io(path, error))?;
                if metadata.is_file() {
                    files += 1;
                    bytes = bytes.saturating_add(metadata.len());
                }
                paths.push(path.to_path_buf());
            }
        } else {
            files = 1;
            bytes = metadata.len();
            paths.push(source.to_path_buf());
        }

        Ok(Self {
            base,
            paths,
            files,
            bytes,
        })
    }
}

fn ensure_available_space(path: &Path, required: u64) -> Result<()> {
    let available = fs2::available_space(path).map_err(|error| VaultError::io(path, error))?;
    if available < required {
        Err(VaultError::InsufficientSpace {
            path: path.to_path_buf(),
            required,
            available,
        })
    } else {
        Ok(())
    }
}

fn estimated_encrypted_size(plain_size: u64) -> u64 {
    let chunks = if plain_size == 0 {
        0
    } else {
        (plain_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64
    };
    let per_chunk_overhead = XCHACHA_NONCE_LEN as u64 + 8 + 16;
    BINARY_BLOB_MAGIC.len() as u64 + 2 + 4 + plain_size + chunks.saturating_mul(per_chunk_overhead)
}

fn write_binary_blob_header(output: &mut impl Write) -> Result<()> {
    output
        .write_all(BINARY_BLOB_MAGIC)
        .map_err(|error| VaultError::io(PathBuf::from("blob header"), error))?;
    output
        .write_all(&BINARY_BLOB_VERSION.to_le_bytes())
        .map_err(|error| VaultError::io(PathBuf::from("blob header"), error))?;
    output
        .write_all(&(CHUNK_SIZE as u32).to_le_bytes())
        .map_err(|error| VaultError::io(PathBuf::from("blob header"), error))?;
    Ok(())
}

fn read_u16(input: &mut impl Read, path: &Path) -> Result<u16> {
    let mut bytes = [0_u8; 2];
    input
        .read_exact(&mut bytes)
        .map_err(|error| VaultError::io(path, error))?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(input: &mut impl Read, path: &Path) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    input
        .read_exact(&mut bytes)
        .map_err(|error| VaultError::io(path, error))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(input: &mut impl Read, path: &Path) -> Result<u64> {
    let mut bytes = [0_u8; 8];
    input
        .read_exact(&mut bytes)
        .map_err(|error| VaultError::io(path, error))?;
    Ok(u64::from_le_bytes(bytes))
}

fn blob_aad_prefix(blob_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(BLOB_AAD_DOMAIN.len() + blob_id.len() + 1);
    aad.extend_from_slice(BLOB_AAD_DOMAIN);
    aad.push(0);
    aad.extend_from_slice(blob_id.as_bytes());
    aad
}

fn blob_chunk_aad(prefix: &[u8], chunk_index: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(prefix.len() + 8);
    aad.extend_from_slice(prefix);
    aad.extend_from_slice(&chunk_index.to_le_bytes());
    aad
}

fn resolve_import_path(
    index: &VaultIndex,
    desired: &VirtualPath,
    kind: EntryKind,
    policy: ImportConflictPolicy,
) -> Result<Option<VirtualPath>> {
    let Some(existing) = index.get(desired) else {
        return Ok(Some(desired.clone()));
    };

    if existing.kind == EntryKind::Directory && kind == EntryKind::Directory {
        return Ok(Some(desired.clone()));
    }

    match policy {
        ImportConflictPolicy::Skip => Ok(None),
        ImportConflictPolicy::Rename => find_available_import_path(index, desired, kind).map(Some),
        ImportConflictPolicy::Overwrite => {
            if existing.kind != kind {
                Err(VaultError::InvalidPath(format!(
                    "cannot overwrite {} with {} at {}",
                    entry_kind_name(existing.kind),
                    entry_kind_name(kind),
                    desired
                )))
            } else {
                Ok(Some(desired.clone()))
            }
        }
    }
}

fn find_available_import_path(
    index: &VaultIndex,
    desired: &VirtualPath,
    kind: EntryKind,
) -> Result<VirtualPath> {
    if index.get(desired).is_none() {
        return Ok(desired.clone());
    }

    let parent = desired.parent().unwrap_or_else(VirtualPath::root);
    let name = desired.file_name().unwrap_or("item");
    let (stem, extension) = if kind == EntryKind::File {
        name.rsplit_once('.')
            .map(|(stem, extension)| (stem.to_string(), format!(".{extension}")))
            .unwrap_or_else(|| (name.to_string(), String::new()))
    } else {
        (name.to_string(), String::new())
    };

    for counter in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({counter}){extension}"))?;
        if index.get(&candidate).is_none() {
            return Ok(candidate);
        }
    }

    Err(VaultError::InvalidPath(format!(
        "could not find a free name for {desired}"
    )))
}

fn entry_kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Directory => "directory",
    }
}

fn cleanup_unreferenced_blobs(root: &Path, index: &VaultIndex) -> Result<usize> {
    let referenced = index
        .list()
        .filter_map(|entry| entry.blob_id.as_deref())
        .collect::<HashSet<_>>();
    let blobs_root = CustomVaultBackend::blobs_path(root);
    if !blobs_root.exists() {
        return Ok(0);
    }

    let mut removed = 0_usize;
    for entry in WalkDir::new(&blobs_root) {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("bin")
        {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !referenced.contains(stem) {
            fs::remove_file(path).map_err(|error| VaultError::io(path, error))?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn cleanup_temp_files(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == TEMP_EXTENSION)
        {
            fs::remove_file(path).map_err(|error| VaultError::io(path, error))?;
        }
    }

    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct EncryptedBlob {
    version: u32,
    chunks: Vec<EncryptedPayload>,
}
