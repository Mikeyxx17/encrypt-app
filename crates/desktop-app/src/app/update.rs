use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::{window, Task};
use secrecy::SecretString;
use vault_core::{
    EntryKind, FailurePolicy, ImportConflictPolicy, ImportOptions, ImportSummary, OperationControl,
    OperationProgress, VaultHealthReport, VirtualPath,
};

use crate::{
    app::{EncryptApp, Message},
    browser::{
        entry_index_in_current_dir, keep_navigation_valid, parent_virtual_path,
        visible_paths_in_current_dir,
    },
    dialogs::{
        confirm_cancel_operation, confirm_cleanup_orphans, confirm_close_while_busy,
        confirm_delete_entries, confirm_move_entries, pick_file, pick_folder,
    },
    formatting::{format_bytes, friendly_error},
    settings::{refresh_vault_records, remember_vault, reveal_in_file_manager, save_vault_records},
};

use super::operations::{
    apply_operation_finished, check_vault_health, create_vault, delete_entries, export_all,
    export_entries, import_path, open_vault,
};
pub(crate) fn update(app: &mut EncryptApp, message: Message) -> Task<Message> {
    match message {
        Message::VaultPathChanged(value) => app.vault_path = value,
        Message::PasswordChanged(value) => app.password = value,
        Message::ImportPathChanged(value) => app.import_path = value,
        Message::ExportPathChanged(value) => app.export_path = value,
        Message::SearchChanged(value) => {
            app.search_query = value;
            app.last_clicked_index = None;
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;
        }
        Message::NewFolderNameChanged(value) => app.new_folder_name = value,
        Message::RenameNameChanged(value) => app.rename_name = value,
        Message::CycleSortMode => {
            app.sort_mode = app.sort_mode.next();
            app.last_clicked_index = None;
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;
        }
        Message::CycleImportConflictPolicy => {
            app.import_conflict_policy = match app.import_conflict_policy {
                ImportConflictPolicy::Rename => ImportConflictPolicy::Skip,
                ImportConflictPolicy::Skip => ImportConflictPolicy::Overwrite,
                ImportConflictPolicy::Overwrite => ImportConflictPolicy::Rename,
            };
            app.status = match app.import_conflict_policy {
                ImportConflictPolicy::Rename => {
                    "同名导入策略：自动改名，会保留旧文件并导入为 “名称 (1)”。".to_string()
                }
                ImportConflictPolicy::Skip => {
                    "同名导入策略：跳过，保险库里已有的同名条目不会被改动。".to_string()
                }
                ImportConflictPolicy::Overwrite => {
                    "同名导入策略：覆盖，会替换保险库里的同名文件内容。".to_string()
                }
            };
        }
        Message::CycleFailurePolicy => {
            app.failure_policy = match app.failure_policy {
                FailurePolicy::StopOnFirstError => FailurePolicy::SkipFailedAndContinue,
                FailurePolicy::SkipFailedAndContinue => FailurePolicy::StopOnFirstError,
            };
            app.status = match app.failure_policy {
                FailurePolicy::StopOnFirstError => "失败策略：遇到错误立即停止。".to_string(),
                FailurePolicy::SkipFailedAndContinue => {
                    "失败策略：跳过失败文件并继续处理。".to_string()
                }
            };
        }
        Message::UseVaultRecord(path) => {
            if !app.busy && app.handle.is_none() {
                app.vault_path = path;
            }
        }
        Message::RemoveVaultRecord(path) => {
            if !app.busy && app.handle.is_none() {
                app.vaults.retain(|record| record.path != path);
                save_vault_records(&app.vaults);
                app.status = "已从列表移除；真实保险库文件未删除。".to_string();
            } else if app.handle.is_some() {
                app.status = "请先锁定保险库再移除记录。".to_string();
            }
        }
        Message::RevealVaultFolder(path) => {
            app.status = match reveal_in_file_manager(Path::new(&path)) {
                Ok(()) => "已打开保险库所在文件夹。".to_string(),
                Err(error) => format!("无法打开所在文件夹：{error}"),
            };
        }
        Message::RefreshVaultRecords => {
            refresh_vault_records(&mut app.vaults);
            save_vault_records(&app.vaults);
            app.status = "保险库列表信息已刷新。".to_string();
        }
        Message::NavigateTo(path) => {
            if !app.busy {
                app.current_dir = path;
                app.selected_entries.clear();
                app.last_clicked_index = None;
                app.last_clicked_path = None;
                app.last_click_time = None;
                app.showing_right_click_picker = false;
                app.right_click_move_source = None;
            }
        }
        Message::NavigateUp => {
            if !app.busy && app.current_dir != "/" {
                app.current_dir = parent_virtual_path(&app.current_dir);
                app.selected_entries.clear();
                app.last_clicked_index = None;
                app.last_clicked_path = None;
                app.last_click_time = None;
                app.showing_right_click_picker = false;
                app.right_click_move_source = None;
            }
        }
        Message::SelectEntry(path) => {
            if app.busy {
                return Task::none();
            }
            // Close right-click picker when user clicks elsewhere
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;

            let now = Instant::now();

            // Double-click detection
            let is_double_click = app.last_clicked_path.as_deref() == Some(&path)
                && app
                    .last_click_time
                    .map(|t| now.duration_since(t).as_millis() < 400)
                    .unwrap_or(false);
            let is_directory = app
                .entries
                .iter()
                .any(|e| e.virtual_path.as_str() == path && e.kind == EntryKind::Directory);

            if is_double_click && is_directory {
                app.current_dir = path;
                app.selected_entries.clear();
                app.last_clicked_index = None;
                app.last_click_time = None;
                app.last_clicked_path = None;
                return Task::none();
            }

            // Shift+Click: range select
            if app.current_modifiers.shift() {
                if let Some(anchor) = app.last_clicked_index {
                    let current_idx = entry_index_in_current_dir(
                        &app.entries,
                        &app.current_dir,
                        &app.search_query,
                        app.sort_mode,
                        &path,
                    );
                    if let Some(current) = current_idx {
                        let start = anchor.min(current);
                        let end = anchor.max(current);
                        let visible = visible_paths_in_current_dir(
                            &app.entries,
                            &app.current_dir,
                            &app.search_query,
                            app.sort_mode,
                        );
                        app.selected_entries.clear();
                        for p in &visible[start..=end] {
                            app.selected_entries.push(p.clone());
                        }
                    }
                } else {
                    app.selected_entries.clear();
                    app.selected_entries.push(path.clone());
                }
            }
            // Ctrl+Click: toggle
            else if app.current_modifiers.control() {
                if let Some(pos) = app.selected_entries.iter().position(|s| s == &path) {
                    app.selected_entries.remove(pos);
                } else {
                    app.selected_entries.push(path.clone());
                }
            }
            // Plain click: deselect if already the only selected, else replace
            else {
                if app.selected_entries.len() == 1 && app.selected_entries[0] == path {
                    app.selected_entries.clear();
                } else {
                    app.selected_entries.clear();
                    app.selected_entries.push(path.clone());
                }
            }

            // Sort + dedup
            app.selected_entries.sort();
            app.selected_entries.dedup();

            // Update tracking state
            app.last_clicked_index = entry_index_in_current_dir(
                &app.entries,
                &app.current_dir,
                &app.search_query,
                app.sort_mode,
                &path,
            );
            app.last_click_time = Some(now);
            app.last_clicked_path = Some(path.clone());

            // Pre-fill rename_name from first selected entry's file_name
            app.rename_name = app
                .selected_entries
                .first()
                .and_then(|s| {
                    app.entries
                        .iter()
                        .find(|e| e.virtual_path.as_str() == s)
                        .and_then(|e| e.virtual_path.file_name())
                })
                .unwrap_or_default()
                .to_string();
        }
        Message::ProgressTick => {
            if let Some(control) = &app.operation_control {
                app.progress = control.snapshot();
            }
        }
        Message::CloseRequested(id) => {
            if app.busy {
                if !app.confirming_close {
                    app.confirming_close = true;
                    return Task::perform(confirm_close_while_busy(), move |confirmed| {
                        Message::CloseRequestConfirmed(id, confirmed)
                    });
                }
            } else {
                return window::close(id);
            }
        }
        Message::CloseRequestConfirmed(id, confirmed) => {
            app.confirming_close = false;
            if confirmed {
                app.pending_close = Some(id);
                if let Some(control) = &app.operation_control {
                    control.cancel();
                    app.status = "正在取消当前操作，清理完成后会关闭窗口。".to_string();
                } else {
                    app.status = "当前操作完成后会关闭窗口。".to_string();
                }
            } else {
                app.status = "已继续当前操作。".to_string();
            }
        }
        Message::RequestCancelOperation => {
            if app.operation_control.is_some() && !app.confirming_cancel {
                app.confirming_cancel = true;
                return Task::perform(
                    confirm_cancel_operation(),
                    Message::CancelOperationConfirmed,
                );
            }
        }
        Message::CancelOperationConfirmed(confirmed) => {
            app.confirming_cancel = false;
            if confirmed {
                if let Some(control) = &app.operation_control {
                    control.cancel();
                    app.status = "正在取消当前操作，清理未完成的临时文件...".to_string();
                }
            } else if app.operation_control.is_some() {
                app.status = "已继续当前操作。".to_string();
            }
        }
        Message::PickVaultFolder => {
            return Task::perform(pick_folder(), Message::PickedVaultFolder);
        }
        Message::PickImportFile => {
            return Task::perform(pick_file(), Message::PickedImportPath);
        }
        Message::PickImportFolder => {
            return Task::perform(pick_folder(), Message::PickedImportPath);
        }
        Message::PickExportFolder => {
            return Task::perform(pick_folder(), Message::PickedExportFolder);
        }
        Message::PickedVaultFolder(path) => {
            if let Some(path) = path {
                app.vault_path = path.display().to_string();
            }
        }
        Message::PickedImportPath(path) => {
            if let Some(path) = path {
                app.import_path = path.display().to_string();
            }
        }
        Message::PickedExportFolder(path) => {
            if let Some(path) = path {
                app.export_path = path.display().to_string();
            }
        }
        Message::CreateVault => {
            let path = PathBuf::from(app.vault_path.trim());
            let password = app.password.clone();
            app.busy = true;
            app.status = "正在创建保险库...".to_string();
            return create_vault(path, password);
        }
        Message::OpenVault => {
            let path = PathBuf::from(app.vault_path.trim());
            let password = app.password.clone();
            app.busy = true;
            app.status = "正在打开保险库...".to_string();
            return open_vault(path, password);
        }
        Message::LockVault => {
            app.handle = None;
            app.entries.clear();
            app.current_dir = "/".to_string();
            app.selected_entries.clear();
            app.last_clicked_index = None;
            app.last_clicked_path = None;
            app.last_click_time = None;
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;
            app.password.clear();
            app.status = "保险库已锁定。".to_string();
        }
        Message::ImportPath => {
            let Some(handle) = app.handle.take() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            let path = PathBuf::from(app.import_path.trim());
            let options = ImportOptions {
                conflict_policy: app.import_conflict_policy,
                failure_policy: app.failure_policy,
            };
            let control = OperationControl::new();
            app.operation_control = Some(control.clone());
            app.progress = OperationProgress::default();
            app.busy = true;
            app.status =
                "正在导入并加密... 如需停止，请点击“取消当前操作”，不要直接关闭窗口。".to_string();
            return import_path(handle, path, options, control);
        }
        Message::CreateFolder => {
            let folder_name = app.new_folder_name.trim();
            if folder_name.is_empty() {
                app.status = "请先输入新文件夹名称。".to_string();
                return Task::none();
            }
            let parent = match VirtualPath::new(&app.current_dir) {
                Ok(path) => path,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let virtual_path = match parent.join(folder_name) {
                Ok(path) => path,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let result = {
                let Some(handle) = app.handle.as_mut() else {
                    app.status = "请先创建或打开保险库。".to_string();
                    return Task::none();
                };
                match handle.create_directory(&virtual_path) {
                    Ok(summary) => Ok((summary, handle.root().to_path_buf(), handle.entries())),
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok((summary, root, entries)) => {
                    app.entries = entries;
                    if let Some(handle) = app.handle.as_ref() {
                        remember_vault(&mut app.vaults, &root, &app.entries, handle);
                    }
                    app.new_folder_name.clear();
                    app.status = format!("已新建文件夹：{}。", virtual_path);
                    if summary.directories == 0 {
                        app.status = "没有新建文件夹。".to_string();
                    }
                }
                Err(error) => app.status = friendly_error(&error.to_string()),
            }
        }
        Message::RenameSelected => {
            if app.selected_entries.len() != 1 {
                app.status = "重命名暂不支持多选".to_string();
                return Task::none();
            }
            let selected = app.selected_entries[0].clone();
            let new_name = app.rename_name.trim().to_string();
            if new_name.is_empty() {
                app.status = "请先输入新名称。".to_string();
                return Task::none();
            }
            let virtual_path = match VirtualPath::new(&selected) {
                Ok(path) => path,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let result = {
                let Some(handle) = app.handle.as_mut() else {
                    app.status = "请先创建或打开保险库。".to_string();
                    return Task::none();
                };
                match handle.rename_entry(&virtual_path, &new_name) {
                    Ok(summary) => Ok((summary, handle.root().to_path_buf(), handle.entries())),
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok((summary, root, entries)) => {
                    app.entries = entries;
                    keep_navigation_valid(app);
                    if let Some(handle) = app.handle.as_ref() {
                        remember_vault(&mut app.vaults, &root, &app.entries, handle);
                    }
                    app.selected_entries.clear();
                    app.last_clicked_index = None;
                    app.rename_name.clear();
                    app.status = format!(
                        "已重命名：{} 个文件，{} 个文件夹。",
                        summary.files, summary.directories
                    );
                }
                Err(error) => app.status = friendly_error(&error.to_string()),
            }
        }
        Message::MoveDestinationChanged(value) => {
            app.move_destination = value;
            app.confirming_move = true;
        }
        Message::RequestMoveSelected => {
            if app.selected_entries.is_empty() {
                app.status = "请先选中要移动的文件或文件夹。".to_string();
                return Task::none();
            };
            let dest_str = app.move_destination.trim();
            if dest_str.is_empty() {
                app.status = "请先输入目标文件夹路径。".to_string();
                return Task::none();
            }
            let dest_dir = match VirtualPath::new(dest_str) {
                Ok(path) => path,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let sources = app.selected_entries.clone();
            return Task::perform(
                confirm_move_entries(sources, dest_dir.to_string()),
                Message::MoveSelectedConfirmed,
            );
        }
        Message::MoveSelectedConfirmed(confirmed) => {
            app.confirming_move = false;
            if !confirmed {
                app.status = "已取消移动。".to_string();
                return Task::none();
            }
            let dest_dir = match VirtualPath::new(app.move_destination.trim()) {
                Ok(path) => path,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let sources = app.selected_entries.clone();
            let Some(handle) = app.handle.as_mut() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            let mut total_files: usize = 0;
            let mut total_dirs: usize = 0;
            let mut errors: Vec<String> = Vec::new();
            for source in &sources {
                let source_path = match VirtualPath::new(source) {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("{source}: {}", friendly_error(&e.to_string())));
                        continue;
                    }
                };
                match handle.move_entry(&source_path, &dest_dir) {
                    Ok(summary) => {
                        total_files += summary.files;
                        total_dirs += summary.directories;
                    }
                    Err(e) => {
                        errors.push(format!("{source}: {}", friendly_error(&e.to_string())));
                    }
                }
            }
            app.entries = handle.entries();
            keep_navigation_valid(app);
            if let Some(h) = app.handle.as_ref() {
                remember_vault(&mut app.vaults, &h.root().to_path_buf(), &app.entries, h);
            }
            app.selected_entries.clear();
            app.last_clicked_index = None;
            app.move_destination.clear();
            let mut status = format!(
                "已移动：{} 个文件，{} 个文件夹，到 {}。",
                total_files, total_dirs, dest_dir
            );
            if !errors.is_empty() {
                status.push_str(&format!("\n{} 个条目移动失败：", errors.len()));
                for err in &errors {
                    status.push_str(&format!("\n  {}", err));
                }
            }
            app.status = status;
        }
        Message::CancelMove => {
            app.confirming_move = false;
            app.move_destination.clear();
        }
        Message::HealthCheck => {
            if let Some(handle) = app.handle.as_ref() {
                let report = handle.check_health();
                let has_issues = health_has_issues(&report);
                app.health_report = Some(report);
                app.showing_health = true;
                app.status = if has_issues {
                    "保险库检查完成：发现问题。".to_string()
                } else {
                    "保险库检查完成：一切正常。".to_string()
                };
            } else {
                if app.vault_path.trim().is_empty() || app.password.is_empty() {
                    app.status = "请先填写保险库路径和密码，再检查保险库。".to_string();
                    return Task::none();
                }
                let path = PathBuf::from(app.vault_path.trim());
                let password = app.password.clone();
                app.busy = true;
                app.status = "正在检查保险库...".to_string();
                return check_vault_health(path, password);
            }
        }
        Message::CleanupOrphans => {
            if app.confirming_cleanup {
                return Task::none();
            }
            let orphan_count = app
                .health_report
                .as_ref()
                .map(|r| r.orphan_blobs.len())
                .unwrap_or(0);
            let reclaimable = app
                .health_report
                .as_ref()
                .map(|r| r.reclaimable_bytes)
                .unwrap_or(0);
            app.confirming_cleanup = true;
            return Task::perform(
                confirm_cleanup_orphans(orphan_count, reclaimable),
                Message::CleanupOrphansConfirmed,
            );
        }
        Message::CleanupOrphansConfirmed(confirmed) => {
            app.confirming_cleanup = false;
            if !confirmed {
                app.status = "已取消清理。".to_string();
                return Task::none();
            }
            let Some(handle) = app.handle.as_ref() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            match handle.cleanup_orphan_blobs() {
                Ok(summary) => {
                    app.health_report = Some(handle.check_health());
                    app.status = format!(
                        "已清理 {} 个无引用密文文件，释放 {} 字节。",
                        summary.removed_count,
                        format_bytes(summary.freed_bytes)
                    );
                }
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                }
            }
        }
        Message::OpenVaultFolder => {
            if let Some(handle) = app.handle.as_ref() {
                if let Err(error) = crate::settings::reveal_in_file_manager(handle.root()) {
                    app.status = format!("无法打开保险库文件夹：{error}");
                }
            } else {
                app.status = "请先创建或打开保险库。".to_string();
            }
        }
        Message::ShowChangePassword => {
            if app.handle.is_some() {
                app.showing_change_password = true;
                app.old_password.clear();
                app.new_password.clear();
                app.new_password_confirm.clear();
            }
        }
        Message::HideChangePassword => {
            app.showing_change_password = false;
            app.old_password.clear();
            app.new_password.clear();
            app.new_password_confirm.clear();
        }
        Message::OldPasswordChanged(value) => app.old_password = value,
        Message::NewPasswordChanged(value) => app.new_password = value,
        Message::NewPasswordConfirmChanged(value) => app.new_password_confirm = value,
        Message::ChangePassword => {
            if app.old_password.is_empty() || app.new_password.is_empty() {
                app.status = "请填写旧密码和新密码。".to_string();
                return Task::none();
            }
            if app.new_password != app.new_password_confirm {
                app.status = "新密码两次输入不一致。".to_string();
                return Task::none();
            }
            let Some(mut handle) = app.handle.take() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            let old_pw = SecretString::new(app.old_password.clone());
            let new_pw = SecretString::new(app.new_password.clone());
            match handle.change_password(&old_pw, &new_pw) {
                Ok(()) => {
                    app.handle = Some(handle);
                    app.showing_change_password = false;
                    app.old_password.clear();
                    app.new_password.clear();
                    app.new_password_confirm.clear();
                    app.password.clear();
                    app.status = "密码已修改成功。".to_string();
                }
                Err(error) => {
                    app.handle = Some(handle);
                    app.status = format!("修改密码失败：{}", friendly_error(&error.to_string()));
                }
            }
        }
        Message::ExportHealthReport => {
            if let Some(report) = &app.health_report {
                let report_json = serde_json::to_string_pretty(report).unwrap_or_default();
                let path = std::path::PathBuf::from("health-report.json");
                match std::fs::write(&path, &report_json) {
                    Ok(()) => {
                        app.status = format!(
                            "检查报告已导出到：{}",
                            path.canonicalize().unwrap_or(path).display()
                        );
                    }
                    Err(error) => {
                        app.status = format!("导出检查报告失败：{error}");
                    }
                }
            } else {
                app.status = "没有检查报告可导出。请先检查保险库。".to_string();
            }
        }
        Message::ExportAll => {
            let Some(handle) = app.handle.take() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            let destination = PathBuf::from(app.export_path.trim());
            let control = OperationControl::new();
            app.operation_control = Some(control.clone());
            app.progress = OperationProgress::default();
            app.busy = true;
            app.status =
                "正在导出并解密... 如需停止，请点击“取消当前操作”，不要直接关闭窗口。".to_string();
            return export_all(handle, destination, app.failure_policy, control);
        }
        Message::ExportSelected => {
            if app.selected_entries.is_empty() {
                app.status = "请先在保险库内容里选中文件或文件夹。".to_string();
                return Task::none();
            };
            if app.export_path.trim().is_empty() {
                app.status = "请先选择导出目标文件夹。".to_string();
                return Task::none();
            }
            let Some(handle) = app.handle.take() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            let destination = PathBuf::from(app.export_path.trim());
            let paths: Vec<VirtualPath> = app
                .selected_entries
                .iter()
                .filter_map(|s| VirtualPath::new(s).ok())
                .collect();
            let control = OperationControl::new();
            app.operation_control = Some(control.clone());
            app.progress = OperationProgress::default();
            app.busy = true;
            let count = paths.len();
            app.status = format!(
                "正在导出并解密 {} 个选中条目。如需停止，请点击“取消当前操作”。",
                count
            );
            return export_entries(handle, paths, destination, app.failure_policy, control);
        }
        Message::RequestDeleteSelected => {
            if app.selected_entries.is_empty() {
                app.status = "请先在保险库内容里选中文件或文件夹。".to_string();
                return Task::none();
            };
            if app.handle.is_some() && !app.busy && !app.confirming_delete {
                let mut total = ImportSummary::default();
                let paths = app.selected_entries.clone();
                for selected in &paths {
                    if let Ok(path) = VirtualPath::new(selected) {
                        if let Some(summary) = app
                            .handle
                            .as_ref()
                            .and_then(|h| h.entry_summary(&path).ok())
                        {
                            total.files += summary.files;
                            total.directories += summary.directories;
                            total.bytes += summary.bytes;
                        }
                    }
                }
                app.confirming_delete = true;
                return Task::perform(
                    confirm_delete_entries(paths, total),
                    Message::DeleteSelectedConfirmed,
                );
            }
        }
        Message::DeleteSelectedConfirmed(confirmed) => {
            app.confirming_delete = false;
            if !confirmed {
                app.status = "已取消删除。".to_string();
                return Task::none();
            }
            let paths: Vec<VirtualPath> = app
                .selected_entries
                .iter()
                .filter_map(|s| VirtualPath::new(s).ok())
                .collect();
            let Some(handle) = app.handle.take() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            app.busy = true;
            app.status = format!("正在从保险库删除 {} 个条目。", paths.len());
            return delete_entries(handle, paths);
        }
        Message::RightClickEntry(path) => {
            if !app.busy && app.handle.is_some() {
                // Also select the entry on right-click
                app.selected_entries.clear();
                app.selected_entries.push(path.clone());
                app.rename_name = app
                    .entries
                    .iter()
                    .find(|e| e.virtual_path.as_str() == &path)
                    .and_then(|e| e.virtual_path.file_name())
                    .unwrap_or_default()
                    .to_string();
                app.last_clicked_index = entry_index_in_current_dir(
                    &app.entries,
                    &app.current_dir,
                    &app.search_query,
                    app.sort_mode,
                    &path,
                );
                app.last_click_time = Some(Instant::now());
                app.last_clicked_path = Some(path.clone());

                app.right_click_move_source = Some(path);
                app.showing_right_click_picker = true;
            }
        }
        Message::PickedRightClickMoveDest(source, dest) => {
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;
            let source_path = match VirtualPath::new(&source) {
                Ok(p) => p,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let dest_dir = match VirtualPath::new(&dest) {
                Ok(p) => p,
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                    return Task::none();
                }
            };
            let Some(handle) = app.handle.as_mut() else {
                app.status = "请先创建或打开保险库。".to_string();
                return Task::none();
            };
            match handle.move_entry(&source_path, &dest_dir) {
                Ok(summary) => {
                    app.entries = handle.entries();
                    keep_navigation_valid(app);
                    if let Some(h) = app.handle.as_ref() {
                        remember_vault(&mut app.vaults, &h.root().to_path_buf(), &app.entries, h);
                    }
                    app.status = format!(
                        "已移动：{} 个文件，{} 个文件夹，到 {}。",
                        summary.files, summary.directories, dest_dir
                    );
                }
                Err(error) => {
                    app.status = friendly_error(&error.to_string());
                }
            }
        }
        Message::CancelRightClickMove => {
            app.showing_right_click_picker = false;
            app.right_click_move_source = None;
        }
        Message::ModifiersChanged(modifiers) => app.current_modifiers = modifiers,
        Message::OperationFinished(result) => {
            return apply_operation_finished(app, result);
        }
    }

    Task::none()
}

fn health_has_issues(report: &VaultHealthReport) -> bool {
    !report.vault_json_exists
        || !report.vault_json_valid
        || !report.index_decryptable
        || !report.missing_blobs.is_empty()
        || !report.orphan_blobs.is_empty()
}
