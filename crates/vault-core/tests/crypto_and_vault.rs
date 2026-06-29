use std::{fs, path::Path};

use secrecy::SecretString;
use vault_core::{
    config::KdfConfig, CryptoProvider, CustomVaultBackend, FailurePolicy, ImportConflictPolicy,
    ImportOptions, OperationControl, Result, VaultBackend, VaultError, VirtualPath,
};

#[test]
fn encryption_roundtrip_and_tamper_detection() -> Result<()> {
    let password = SecretString::new("correct horse battery staple".to_string());
    let kdf = KdfConfig::new_interactive();
    let key = CryptoProvider::derive_key(&password, &kdf)?;
    let payload = CryptoProvider::encrypt(&key, b"hello vault")?;
    let plaintext = CryptoProvider::decrypt(&key, &payload)?;
    assert_eq!(plaintext.as_slice(), b"hello vault");

    let mut tampered = payload.clone();
    tampered.ciphertext_b64.push('A');
    assert!(matches!(
        CryptoProvider::decrypt(&key, &tampered),
        Err(VaultError::TamperedCiphertext | VaultError::InvalidFormat(_))
    ));

    let second = CryptoProvider::encrypt(&key, b"hello vault")?;
    assert_ne!(payload.nonce_b64, second.nonce_b64);
    assert_ne!(payload.ciphertext_b64, second.ciphertext_b64);
    Ok(())
}

#[test]
fn wrong_password_cannot_open_vault() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("right".to_string()))?;
    drop(handle);

    let result = CustomVaultBackend::open(&vault_path, SecretString::new("wrong".to_string()));
    assert!(matches!(result, Err(VaultError::InvalidPassword)));
    Ok(())
}

#[test]
fn change_password_rewraps_key_and_keeps_backup() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"secret").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("old-password".to_string()))?;
    handle.import_path(&input)?;

    handle.change_password(
        &SecretString::new("old-password".to_string()),
        &SecretString::new("new-password".to_string()),
    )?;
    assert!(vault_path.join("vault.json.bak").exists());
    drop(handle);

    let old_result =
        CustomVaultBackend::open(&vault_path, SecretString::new("old-password".to_string()));
    assert!(matches!(old_result, Err(VaultError::InvalidPassword)));

    let handle =
        CustomVaultBackend::open(&vault_path, SecretString::new("new-password".to_string()))?;
    let output = dir.path().join("output");
    handle.export_all(&output)?;
    assert_file_eq(&input, output.join("secret.txt"));
    Ok(())
}

#[test]
fn change_password_rejects_wrong_old_password_without_rewriting_config() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("old-password".to_string()))?;
    let before = fs::read_to_string(vault_path.join("vault.json")).expect("read config");

    let result = handle.change_password(
        &SecretString::new("wrong-password".to_string()),
        &SecretString::new("new-password".to_string()),
    );
    assert!(matches!(result, Err(VaultError::InvalidPassword)));
    let after = fs::read_to_string(vault_path.join("vault.json")).expect("read config");
    assert_eq!(before, after);
    drop(handle);

    let handle =
        CustomVaultBackend::open(&vault_path, SecretString::new("old-password".to_string()))?;
    drop(handle);
    let new_result =
        CustomVaultBackend::open(&vault_path, SecretString::new("new-password".to_string()));
    assert!(matches!(new_result, Err(VaultError::InvalidPassword)));
    Ok(())
}

#[test]
fn import_export_folder_roundtrip_preserves_content() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("input");
    fs::create_dir_all(input.join("nested")).expect("input dirs");
    fs::write(input.join("hello.txt"), b"hello").expect("write hello");
    fs::write(input.join("nested").join("unicode-文件.txt"), b"world").expect("write nested");
    fs::write(input.join("empty.bin"), b"").expect("write empty");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    let summary = handle.import_path(&input)?;
    assert_eq!(summary.files, 3);
    assert!(summary.directories >= 1);
    assert!(handle
        .index()
        .get(&VirtualPath::new("/nested/unicode-文件.txt")?)
        .is_some());

    drop(handle);
    let handle = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()))?;
    let output = dir.path().join("output");
    handle.export_all(&output)?;

    assert_file_eq(input.join("hello.txt"), output.join("hello.txt"));
    assert_file_eq(
        input.join("nested").join("unicode-文件.txt"),
        output.join("nested").join("unicode-文件.txt"),
    );
    assert_file_eq(input.join("empty.bin"), output.join("empty.bin"));
    Ok(())
}

#[test]
fn import_encrypts_storage_without_plaintext_copy() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"plain text should not appear in the vault root").expect("write secret");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;

    assert!(!vault_path.join("secret.txt").exists());

    let index = fs::read_to_string(vault_path.join("index.enc")).expect("read encrypted index");
    assert!(!index.contains("secret.txt"));
    assert!(!index.contains("plain text should not appear"));

    let blob_names = collect_file_names(vault_path.join("blobs"));
    assert!(blob_names.iter().all(|name| !name.contains("secret")));

    let blob_files = collect_files(vault_path.join("blobs"));
    assert_eq!(blob_files.len(), 1);
    let blob = fs::read(&blob_files[0]).expect("read blob");
    assert!(blob.starts_with(b"ZEVBLOB2"));
    Ok(())
}

#[test]
fn multi_chunk_file_roundtrip_uses_streaming_binary_blob() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("video-like.bin");
    let mut data = Vec::with_capacity(3 * 1024 * 1024 + 123);
    for index in 0..(3 * 1024 * 1024 + 123) {
        data.push((index % 251) as u8);
    }
    fs::write(&input, &data).expect("write large-ish file");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;

    let blob_files = collect_files(vault_path.join("blobs"));
    assert_eq!(blob_files.len(), 1);
    let blob = fs::read(&blob_files[0]).expect("read blob");
    assert!(blob.starts_with(b"ZEVBLOB2"));
    assert!(!blob.windows(16).any(|window| window == &data[..16]));

    let output = dir.path().join("output");
    handle.export_all(&output)?;
    assert_file_eq(&input, output.join("video-like.bin"));
    Ok(())
}

#[test]
fn export_to_vault_folder_is_rejected() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("requirements.txt");
    fs::write(&input, b"package==1.0").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;

    let result = handle.export_all(&vault_path);
    assert!(matches!(
        result,
        Err(VaultError::ExportDestinationInsideVault { .. })
    ));
    assert!(!vault_path.join("requirements.txt").exists());
    Ok(())
}

#[test]
fn import_from_vault_folder_is_rejected() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    let accidental_plaintext = vault_path.join("requirements.txt");
    fs::write(&accidental_plaintext, b"do not import from vault").expect("write accidental file");

    let result = handle.import_path(&accidental_plaintext);
    assert!(matches!(
        result,
        Err(VaultError::ImportSourceInsideVault { .. })
    ));
    Ok(())
}

#[test]
fn stale_import_temp_files_are_cleaned_when_opening_vault() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    drop(handle);

    let temp_dir = vault_path.join("blobs").join("aa");
    fs::create_dir_all(&temp_dir).expect("temp dir");
    let stale = temp_dir.join("leftover.encrypt-app-tmp");
    fs::write(&stale, b"partial ciphertext").expect("write stale temp");
    assert!(stale.exists());

    let _handle = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()))?;
    assert!(!stale.exists());
    Ok(())
}

#[test]
fn vault_lock_prevents_second_open_until_handle_is_dropped() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;

    let result = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()));
    assert!(matches!(result, Err(VaultError::VaultLocked { .. })));

    drop(handle);
    let reopened =
        CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()))?;
    drop(reopened);
    Ok(())
}

#[test]
fn missing_blob_is_reported_when_opening_vault() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"secret").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;
    let blob_path = handle
        .index()
        .get(&VirtualPath::new("/secret.txt")?)
        .and_then(|entry| entry.blob_id.as_ref())
        .map(|blob_id| {
            let prefix = blob_id.get(0..2).unwrap_or("00");
            vault_path
                .join("blobs")
                .join(prefix)
                .join(format!("{blob_id}.bin"))
        })
        .expect("blob path");
    fs::remove_file(blob_path).expect("remove blob");
    drop(handle);

    let result = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()));
    assert!(matches!(result, Err(VaultError::MissingBlob { .. })));
    Ok(())
}

#[test]
fn export_can_skip_failed_files_and_continue() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("input");
    fs::create_dir_all(&input).expect("input dir");
    fs::write(input.join("keep.txt"), b"keep").expect("write keep");
    fs::write(input.join("missing.txt"), b"missing").expect("write missing");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;

    let blob_path = handle
        .index()
        .get(&VirtualPath::new("/missing.txt")?)
        .and_then(|entry| entry.blob_id.as_ref())
        .map(|blob_id| {
            let prefix = blob_id.get(0..2).unwrap_or("00");
            vault_path
                .join("blobs")
                .join(prefix)
                .join(format!("{blob_id}.bin"))
        })
        .expect("blob path");
    fs::remove_file(blob_path).expect("remove blob");

    let output = dir.path().join("output");
    let control = OperationControl::new();
    let summary = handle.export_all_with_policy_and_control(
        &output,
        FailurePolicy::SkipFailedAndContinue,
        &control,
    )?;
    let report = control.report_snapshot();

    assert_eq!(summary.files, 1);
    assert_eq!(report.files_skipped, 1);
    assert_eq!(report.issues.len(), 1);
    assert_file_eq(input.join("keep.txt"), output.join("keep.txt"));
    assert!(!output.join("missing.txt").exists());
    Ok(())
}

#[test]
fn directory_rename_duplicate_import_and_cleanup_work() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("document.txt");
    fs::write(&input, b"first").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.create_directory(&VirtualPath::new("/docs")?)?;
    handle.rename_entry(&VirtualPath::new("/docs")?, "renamed")?;
    assert!(handle.index().get(&VirtualPath::new("/renamed")?).is_some());

    let summary = handle.import_path_with_options(
        &input,
        ImportOptions {
            conflict_policy: ImportConflictPolicy::Rename,
            ..Default::default()
        },
    )?;
    assert_eq!(summary.files, 1);
    let summary = handle.import_path_with_options(
        &input,
        ImportOptions {
            conflict_policy: ImportConflictPolicy::Rename,
            ..Default::default()
        },
    )?;
    assert_eq!(summary.files, 1);
    assert!(handle
        .index()
        .get(&VirtualPath::new("/document.txt")?)
        .is_some());
    assert!(handle
        .index()
        .get(&VirtualPath::new("/document (1).txt")?)
        .is_some());

    let extra = vault_path.join("blobs").join("ff").join("ffffffff.bin");
    fs::create_dir_all(extra.parent().expect("extra parent")).expect("extra dir");
    fs::write(&extra, b"orphan").expect("write orphan");
    assert!(extra.exists());
    assert_eq!(handle.cleanup_unreferenced_blobs()?, 1);
    assert!(!extra.exists());
    Ok(())
}

#[test]
fn index_backup_is_used_if_primary_index_is_missing() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"hello").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;
    drop(handle);

    fs::copy(
        vault_path.join("index.enc"),
        vault_path.join("index.enc.bak"),
    )
    .expect("copy backup");
    fs::remove_file(vault_path.join("index.enc")).expect("remove primary index");

    let handle = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()))?;
    assert!(handle
        .index()
        .get(&VirtualPath::new("/secret.txt")?)
        .is_some());
    Ok(())
}

#[test]
fn stale_export_temp_files_are_cleaned_before_export() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"hello").expect("write input");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;

    let output = dir.path().join("output");
    fs::create_dir_all(&output).expect("output dir");
    let stale = output.join("secret.encrypt-app-tmp");
    fs::write(&stale, b"partial plaintext").expect("write stale temp");
    assert!(stale.exists());

    handle.export_all(&output)?;
    assert!(!stale.exists());
    assert_file_eq(&input, output.join("secret.txt"));
    Ok(())
}

#[test]
fn delete_entry_removes_index_entries_and_blobs() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("input");
    fs::create_dir_all(input.join("nested")).expect("input dirs");
    fs::write(input.join("keep.txt"), b"keep").expect("write keep");
    fs::write(input.join("nested").join("remove.txt"), b"remove").expect("write remove");

    let vault_path = dir.path().join("vault");
    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))?;
    handle.import_path(&input)?;
    assert_eq!(collect_files(vault_path.join("blobs")).len(), 2);

    let summary = handle.delete_entry(&VirtualPath::new("/nested")?)?;
    assert_eq!(summary.files, 1);
    assert_eq!(summary.directories, 1);
    assert_eq!(summary.bytes, 6);
    assert!(handle.index().get(&VirtualPath::new("/nested")?).is_none());
    assert!(handle
        .index()
        .get(&VirtualPath::new("/nested/remove.txt")?)
        .is_none());
    assert_eq!(collect_files(vault_path.join("blobs")).len(), 1);

    drop(handle);
    let handle = CustomVaultBackend::open(&vault_path, SecretString::new("password".to_string()))?;
    let output = dir.path().join("output");
    handle.export_all(&output)?;
    assert_file_eq(input.join("keep.txt"), output.join("keep.txt"));
    assert!(!output.join("nested").exists());
    Ok(())
}

#[test]
fn virtual_paths_are_normalized_and_export_safe() -> Result<()> {
    assert_eq!(VirtualPath::new(r"\a\b\c.txt")?.as_str(), "/a/b/c.txt");
    assert!(VirtualPath::new("/a/../b").is_err());
    assert_eq!(
        VirtualPath::new("/CON")?.to_safe_os_path(),
        Path::new("CON_")
    );
    assert_eq!(
        VirtualPath::new("/bad:name?.txt")?.to_safe_os_path(),
        Path::new("bad_name_.txt")
    );
    Ok(())
}

fn assert_file_eq(left: impl AsRef<Path>, right: impl AsRef<Path>) {
    let left = fs::read(left).expect("read left");
    let right = fs::read(right).expect("read right");
    assert_eq!(left, right);
}

fn collect_file_names(root: impl AsRef<Path>) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root).expect("read blobs") {
        let entry = entry.expect("blob prefix");
        if entry.path().is_dir() {
            names.extend(collect_file_names(entry.path()));
        } else {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names
}

fn collect_files(root: impl AsRef<Path>) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root).expect("read directory") {
        let entry = entry.expect("directory entry");
        if entry.path().is_dir() {
            files.extend(collect_files(entry.path()));
        } else {
            files.push(entry.path());
        }
    }
    files
}
