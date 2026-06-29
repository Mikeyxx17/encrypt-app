use std::path::PathBuf;

use iced::{window, Task};
use secrecy::SecretString;
use vault_core::{
    CustomVaultBackend, FailurePolicy, ImportOptions, ImportSummary, OperationControl,
    VaultBackend, VaultEntry, VaultHandle, VaultHealthReport, VirtualPath,
};

use crate::{
    app::{EncryptApp, Message},
    browser::keep_navigation_valid,
    formatting::{format_bytes, friendly_error},
    settings::remember_vault,
};
#[derive(Debug, Clone)]
pub(crate) enum OperationResult {
    Created(ResultData),
    Opened(ResultData),
    Imported(ResultData, ImportSummary),
    Exported(ResultData, ImportSummary),
    Deleted(ResultData, ImportSummary),
    HealthChecked(VaultHealthReport),
    Failed(String, Option<ResultData>),
}

#[derive(Debug, Clone)]
pub(crate) struct ResultData {
    pub(crate) path: PathBuf,
    pub(crate) entries: Vec<VaultEntry>,
    pub(crate) handle: VaultHandle,
}

pub(crate) fn create_vault(path: PathBuf, password: String) -> Task<Message> {
    Task::perform(
        async move {
            match CustomVaultBackend::create(&path, SecretString::new(password)) {
                Ok(handle) => {
                    let entries = handle.entries();
                    OperationResult::Created(ResultData {
                        path,
                        entries,
                        handle,
                    })
                }
                Err(error) => OperationResult::Failed(error.to_string(), None),
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn open_vault(path: PathBuf, password: String) -> Task<Message> {
    Task::perform(
        async move {
            match CustomVaultBackend::open(&path, SecretString::new(password)) {
                Ok(handle) => {
                    let entries = handle.entries();
                    OperationResult::Opened(ResultData {
                        path,
                        entries,
                        handle,
                    })
                }
                Err(error) => OperationResult::Failed(error.to_string(), None),
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn import_path(
    mut handle: VaultHandle,
    path: PathBuf,
    options: ImportOptions,
    control: OperationControl,
) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                match handle.import_path_with_options_and_control(&path, options, &control) {
                    Ok(summary) => {
                        let entries = handle.entries();
                        OperationResult::Imported(
                            ResultData {
                                path,
                                entries,
                                handle,
                            },
                            summary,
                        )
                    }
                    Err(error) => {
                        let entries = handle.entries();
                        OperationResult::Failed(
                            error.to_string(),
                            Some(ResultData {
                                path,
                                entries,
                                handle,
                            }),
                        )
                    }
                }
            })
            .await;

            match result {
                Ok(result) => result,
                Err(error) => {
                    OperationResult::Failed(format!("background import failed: {error}"), None)
                }
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn export_all(
    handle: VaultHandle,
    destination: PathBuf,
    failure_policy: FailurePolicy,
    control: OperationControl,
) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let result = handle.export_all_with_policy_and_control(
                    &destination,
                    failure_policy,
                    &control,
                );
                let entries = handle.entries();
                match result {
                    Ok(summary) => OperationResult::Exported(
                        ResultData {
                            path: destination,
                            entries,
                            handle,
                        },
                        summary,
                    ),
                    Err(error) => OperationResult::Failed(
                        error.to_string(),
                        Some(ResultData {
                            path: destination,
                            entries,
                            handle,
                        }),
                    ),
                }
            })
            .await;

            match result {
                Ok(result) => result,
                Err(error) => {
                    OperationResult::Failed(format!("background export failed: {error}"), None)
                }
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn export_entries(
    handle: VaultHandle,
    paths: Vec<VirtualPath>,
    destination: PathBuf,
    failure_policy: FailurePolicy,
    control: OperationControl,
) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut total_files: usize = 0;
                let mut total_dirs: usize = 0;
                let mut total_bytes: u64 = 0;
                let mut errors: Vec<String> = Vec::new();
                for path in &paths {
                    if control.is_cancelled() {
                        errors.push("操作已取消".to_string());
                        break;
                    }
                    match handle.export_entry_with_policy_and_control(
                        path,
                        &destination,
                        failure_policy,
                        &control,
                    ) {
                        Ok(summary) => {
                            total_files += summary.files;
                            total_dirs += summary.directories;
                            total_bytes += summary.bytes;
                        }
                        Err(e) => {
                            if failure_policy == FailurePolicy::SkipFailedAndContinue {
                                control.add_failed();
                                control.add_issue(&path.to_string(), &e.to_string());
                                continue;
                            }
                            errors.push(format!("{}: {}", path, e));
                        }
                    }
                }
                let entries = handle.entries();
                if errors.is_empty() {
                    OperationResult::Exported(
                        ResultData {
                            path: destination,
                            entries,
                            handle,
                        },
                        ImportSummary {
                            files: total_files,
                            directories: total_dirs,
                            bytes: total_bytes,
                        },
                    )
                } else {
                    OperationResult::Failed(
                        errors.join("; "),
                        Some(ResultData {
                            path: destination,
                            entries,
                            handle,
                        }),
                    )
                }
            })
            .await;

            match result {
                Ok(result) => result,
                Err(error) => {
                    OperationResult::Failed(format!("background export failed: {error}"), None)
                }
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn check_vault_health(path: PathBuf, password: String) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                CustomVaultBackend::check_health(&path, SecretString::new(password))
            })
            .await;

            match result {
                Ok(Ok(report)) => OperationResult::HealthChecked(report),
                Ok(Err(error)) => OperationResult::Failed(error.to_string(), None),
                Err(error) => OperationResult::Failed(
                    format!("background health check failed: {error}"),
                    None,
                ),
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn delete_entries(mut handle: VaultHandle, paths: Vec<VirtualPath>) -> Task<Message> {
    Task::perform(
        async move {
            let root = handle.root().to_path_buf();
            let mut total_files: usize = 0;
            let mut total_dirs: usize = 0;
            let mut total_bytes: u64 = 0;
            let mut errors: Vec<String> = Vec::new();
            for path in &paths {
                match handle.delete_entry(path) {
                    Ok(summary) => {
                        total_files += summary.files;
                        total_dirs += summary.directories;
                        total_bytes += summary.bytes;
                    }
                    Err(error) => {
                        errors.push(format!("{}: {}", path, error));
                    }
                }
            }
            let entries = handle.entries();
            if errors.is_empty() {
                OperationResult::Deleted(
                    ResultData {
                        path: root,
                        entries,
                        handle,
                    },
                    ImportSummary {
                        files: total_files,
                        directories: total_dirs,
                        bytes: total_bytes,
                    },
                )
            } else {
                OperationResult::Failed(
                    errors.join("; "),
                    Some(ResultData {
                        path: root,
                        entries,
                        handle,
                    }),
                )
            }
        },
        Message::OperationFinished,
    )
}

pub(crate) fn apply_operation_finished(
    app: &mut EncryptApp,
    result: OperationResult,
) -> Task<Message> {
    let report = app
        .operation_control
        .as_ref()
        .map(|c| c.report_snapshot())
        .unwrap_or_default();
    app.busy = false;
    app.operation_control = None;
    app.confirming_cancel = false;
    app.confirming_close = false;
    app.confirming_delete = false;
    app.showing_right_click_picker = false;
    app.right_click_move_source = None;
    match result {
        OperationResult::Created(data) => {
            remember_vault(&mut app.vaults, &data.path, &data.entries, &data.handle);
            app.entries = data.entries;
            app.handle = Some(data.handle);
            app.current_dir = "/".to_string();
            app.selected_entries.clear();
            app.last_clicked_index = None;
            app.password.clear();
            app.status = format!("保险库已创建：{}", data.path.display());
        }
        OperationResult::Opened(data) => {
            remember_vault(&mut app.vaults, &data.path, &data.entries, &data.handle);
            app.entries = data.entries;
            app.handle = Some(data.handle);
            app.current_dir = "/".to_string();
            app.selected_entries.clear();
            app.last_clicked_index = None;
            app.password.clear();
            app.status = format!("保险库已打开：{}", data.path.display());
        }
        OperationResult::Imported(data, summary) => {
            let root = data.handle.root().to_path_buf();
            remember_vault(&mut app.vaults, &root, &data.entries, &data.handle);
            app.entries = data.entries;
            keep_navigation_valid(app);
            app.handle = Some(data.handle);
            let mut status = format!(
                "已加密保存：{} 个文件，{} 个文件夹，{} 字节。原文件已保留。",
                summary.files, summary.directories, summary.bytes
            );
            if report.files_skipped > 0 {
                status.push_str(&format!(
                    "\n跳过 {} 个失败文件（{} 字节）：",
                    report.files_skipped, report.bytes_skipped
                ));
                for issue in &report.issues {
                    status.push_str(&format!("\n  {} — {}", issue.path, issue.error));
                }
            }
            app.status = status;
        }
        OperationResult::Exported(data, summary) => {
            app.entries = data.entries;
            app.handle = Some(data.handle);
            let mut status = format!(
                "已导出：{} 个文件，{} 个文件夹，{} 字节，到 {}。",
                summary.files,
                summary.directories,
                summary.bytes,
                data.path.display()
            );
            if report.files_skipped > 0 {
                status.push_str(&format!(
                    "\n跳过 {} 个失败文件（{} 字节）：",
                    report.files_skipped, report.bytes_skipped
                ));
                for issue in &report.issues {
                    status.push_str(&format!("\n  {} — {}", issue.path, issue.error));
                }
            }
            app.status = status;
        }
        OperationResult::Deleted(data, summary) => {
            remember_vault(&mut app.vaults, &data.path, &data.entries, &data.handle);
            app.entries = data.entries;
            app.selected_entries.clear();
            app.last_clicked_index = None;
            keep_navigation_valid(app);
            app.handle = Some(data.handle);
            app.status = format!(
                "已从保险库删除：{} 个文件，{} 个文件夹，{}。保险库外的原文件不会受影响。",
                summary.files,
                summary.directories,
                format_bytes(summary.bytes)
            );
        }
        OperationResult::HealthChecked(report) => {
            let has_issues = !report.vault_json_exists
                || !report.vault_json_valid
                || !report.index_decryptable
                || !report.missing_blobs.is_empty()
                || !report.orphan_blobs.is_empty();
            app.health_report = Some(report);
            app.showing_health = true;
            app.status = if has_issues {
                "保险库检查完成：发现问题。".to_string()
            } else {
                "保险库检查完成：一切正常。".to_string()
            };
        }
        OperationResult::Failed(error, data) => {
            if let Some(data) = data {
                app.entries = data.entries;
                keep_navigation_valid(app);
                app.handle = Some(data.handle);
            }
            app.status = friendly_error(&error);
        }
    }

    if let Some(id) = app.pending_close.take() {
        return window::close(id);
    }

    Task::none()
}
