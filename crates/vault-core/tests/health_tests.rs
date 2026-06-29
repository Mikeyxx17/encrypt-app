use std::fs;

use secrecy::SecretString;
use vault_core::{CustomVaultBackend, VaultBackend};

#[test]
fn health_check_healthy_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");

    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
        .expect("create vault");

    let report = handle.check_health();
    assert!(report.vault_json_exists);
    assert!(report.vault_json_valid);
    assert!(report.index_decryptable);
    assert_eq!(report.total_files_in_index, 0);
    assert!(report.missing_blobs.is_empty());
    assert!(report.orphan_blobs.is_empty());
    assert_eq!(report.reclaimable_bytes, 0);
}

#[test]
fn health_check_finds_missing_blob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let input = dir.path().join("input");
    fs::create_dir_all(&input).expect("create input");
    fs::write(input.join("test.txt"), b"hello world").expect("write file");

    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
            .expect("create vault");
    handle
        .import_path(&input.join("test.txt"))
        .expect("import file");

    // Delete the blob to simulate a missing blob
    let entry = handle
        .entries()
        .into_iter()
        .find(|e| e.blob_id.is_some())
        .expect("find file entry");
    let blob_id = entry.blob_id.as_ref().unwrap();
    let prefix = blob_id.get(0..2).unwrap_or("00");
    let blob_path = vault_path
        .join("blobs")
        .join(prefix)
        .join(format!("{blob_id}.bin"));
    fs::remove_file(&blob_path).expect("remove blob");

    let report = handle.check_health();
    assert!(report.vault_json_exists);
    assert!(report.vault_json_valid);
    assert_eq!(report.total_files_in_index, 1);
    assert_eq!(report.missing_blobs.len(), 1);
    assert_eq!(report.missing_blobs[0].virtual_path, "/test.txt");
    assert_eq!(report.missing_blobs[0].blob_id, *blob_id);
}

#[test]
fn unopened_health_check_finds_missing_blob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let input = dir.path().join("input");
    fs::create_dir_all(&input).expect("create input");
    fs::write(input.join("test.txt"), b"hello world").expect("write file");

    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
            .expect("create vault");
    handle
        .import_path(&input.join("test.txt"))
        .expect("import file");

    let entry = handle
        .entries()
        .into_iter()
        .find(|e| e.blob_id.is_some())
        .expect("find file entry");
    let blob_id = entry.blob_id.as_ref().unwrap();
    let prefix = blob_id.get(0..2).unwrap_or("00");
    let blob_path = vault_path
        .join("blobs")
        .join(prefix)
        .join(format!("{blob_id}.bin"));
    fs::remove_file(&blob_path).expect("remove blob");
    drop(handle);

    let report =
        CustomVaultBackend::check_health(&vault_path, SecretString::new("password".to_string()))
            .expect("offline health check");
    assert!(report.index_decryptable);
    assert_eq!(report.total_files_in_index, 1);
    assert_eq!(report.missing_blobs.len(), 1);
    assert_eq!(report.missing_blobs[0].virtual_path, "/test.txt");
}

#[test]
fn unopened_health_check_reports_corrupt_index() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");

    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
        .expect("create vault");
    drop(handle);

    fs::write(vault_path.join("index.enc"), b"not valid encrypted index").expect("corrupt index");

    let report =
        CustomVaultBackend::check_health(&vault_path, SecretString::new("password".to_string()))
            .expect("offline health check");
    assert!(report.vault_json_exists);
    assert!(report.vault_json_valid);
    assert!(!report.index_decryptable);
    assert_eq!(report.total_files_in_index, 0);
    assert!(report.missing_blobs.is_empty());
    assert!(report.orphan_blobs.is_empty());
}

#[test]
fn health_check_validates_vault_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");

    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
        .expect("create vault");
    drop(handle);

    let config_path = vault_path.join("vault.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).expect("read config"))
            .expect("parse config");
    config["format_id"] = serde_json::Value::String("wrong-format".to_string());
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).expect("serialize config"),
    )
    .expect("write invalid config");

    let report =
        CustomVaultBackend::check_health(&vault_path, SecretString::new("password".to_string()))
            .expect("offline health check");
    assert!(report.vault_json_exists);
    assert!(!report.vault_json_valid);
    assert!(!report.index_decryptable);
}

#[test]
fn health_check_finds_orphan_blob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");

    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
        .expect("create vault");

    // Create an orphan .bin file manually
    let orphan_dir = vault_path.join("blobs").join("aa");
    fs::create_dir_all(&orphan_dir).expect("create orphan dir");
    fs::write(
        orphan_dir.join("deadbeef00deadbeef00deadbeef00deadbeef00.bin"),
        b"orphan data",
    )
    .expect("write orphan");

    let report = handle.check_health();
    assert!(report.vault_json_exists);
    assert_eq!(report.orphan_blobs.len(), 1);
    assert!(report.reclaimable_bytes > 0);
}

#[test]
fn cleanup_only_removes_orphans() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");
    let input = dir.path().join("input");
    fs::create_dir_all(&input).expect("create input");
    fs::write(input.join("keep.txt"), b"keep me").expect("write file");

    let mut handle =
        CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
            .expect("create vault");
    handle
        .import_path(&input.join("keep.txt"))
        .expect("import file");

    // Create an orphan blob
    let orphan_dir = vault_path.join("blobs").join("bb");
    fs::create_dir_all(&orphan_dir).expect("create orphan dir");
    fs::write(
        orphan_dir.join("ffffffffffffffffffffffffffffffffffffffff.bin"),
        b"orphan garbage",
    )
    .expect("write orphan");

    let report_before = handle.check_health();
    assert_eq!(report_before.orphan_blobs.len(), 1);

    let summary = handle.cleanup_orphan_blobs().expect("cleanup orphans");
    assert_eq!(summary.removed_count, 1);
    assert!(summary.freed_bytes > 0);

    let report_after = handle.check_health();
    assert!(report_after.orphan_blobs.is_empty());
    assert_eq!(report_after.missing_blobs.len(), 0);
    assert_eq!(report_after.total_files_in_index, 1);
}

#[test]
fn empty_vault_health_is_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault_path = dir.path().join("vault");

    let handle = CustomVaultBackend::create(&vault_path, SecretString::new("password".to_string()))
        .expect("create vault");

    let summary = handle.cleanup_orphan_blobs().expect("cleanup");
    assert_eq!(summary.removed_count, 0);
    assert_eq!(summary.freed_bytes, 0);

    let report = handle.check_health();
    assert_eq!(report.total_files_in_index, 0);
    assert!(report.missing_blobs.is_empty());
    assert!(report.orphan_blobs.is_empty());
}
