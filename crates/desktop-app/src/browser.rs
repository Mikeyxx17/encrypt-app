use vault_core::{EntryKind, VaultEntry};

use crate::{
    app::{EncryptApp, SortMode},
    formatting::{format_bytes, format_display_time},
};
pub(crate) fn selected_entries<'a>(app: &'a EncryptApp) -> Vec<&'a VaultEntry> {
    app.selected_entries
        .iter()
        .filter_map(|path| {
            app.entries
                .iter()
                .find(|entry| entry.virtual_path.as_str() == path)
        })
        .collect()
}

pub(crate) fn is_entry_selected(app: &EncryptApp, path: &str) -> bool {
    app.selected_entries.iter().any(|s| s == path)
}

pub(crate) fn current_dir_entries<'a>(
    entries: &'a [VaultEntry],
    current_dir: &str,
    search_query: &str,
    sort_mode: SortMode,
) -> Vec<&'a VaultEntry> {
    let query = search_query.trim().to_lowercase();
    let mut visible = entries
        .iter()
        .filter(|entry| entry_parent_path(entry.virtual_path.as_str()) == current_dir)
        .filter(|entry| {
            query.is_empty()
                || entry
                    .virtual_path
                    .as_str()
                    .to_lowercase()
                    .contains(query.as_str())
        })
        .collect::<Vec<_>>();
    visible.sort_by(|left, right| {
        (left.kind == EntryKind::File)
            .cmp(&(right.kind == EntryKind::File))
            .then_with(|| match sort_mode {
                SortMode::NameAsc => entry_name(left).cmp(&entry_name(right)),
                SortMode::NameDesc => entry_name(right).cmp(&entry_name(left)),
                SortMode::SizeDesc => right.size.cmp(&left.size),
                SortMode::ModifiedDesc => right.modified_at.cmp(&left.modified_at),
            })
    });
    visible
}

pub(crate) fn keep_navigation_valid(app: &mut EncryptApp) {
    if app.current_dir != "/"
        && !app.entries.iter().any(|entry| {
            entry.kind == EntryKind::Directory && entry.virtual_path.as_str() == app.current_dir
        })
    {
        app.current_dir = "/".to_string();
    }

    app.selected_entries.retain(|s| {
        app.entries
            .iter()
            .any(|entry| entry.virtual_path.as_str() == s)
    });
    if app.selected_entries.is_empty() {
        app.last_clicked_index = None;
    }
}

pub(crate) fn can_export_selected(app: &EncryptApp) -> bool {
    app.handle.is_some()
        && !app.busy
        && !app.selected_entries.is_empty()
        && !app.export_path.trim().is_empty()
}

pub(crate) fn vault_totals(entries: &[VaultEntry]) -> (usize, usize, u64) {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .count();
    let directories = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Directory)
        .count();
    let bytes = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.size)
        .sum();
    (files, directories, bytes)
}

pub(crate) fn entry_kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "文件",
        EntryKind::Directory => "文件夹",
    }
}

pub(crate) fn entry_name(entry: &VaultEntry) -> String {
    entry.virtual_path.file_name().unwrap_or("/").to_string()
}

pub(crate) fn entry_size_label(entry: &VaultEntry) -> String {
    match entry.kind {
        EntryKind::File => format_bytes(entry.size),
        EntryKind::Directory => "-".to_string(),
    }
}

pub(crate) fn entry_modified_label(entry: &VaultEntry) -> String {
    entry
        .modified_at
        .as_ref()
        .map(|time| format_display_time(&time.to_rfc3339()))
        .unwrap_or_else(|| "-".to_string())
}

pub(crate) fn entry_parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed == "/" {
        return "/".to_string();
    }

    match trimmed.rsplit_once('/') {
        Some(("", _)) | None => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
    }
}

pub(crate) fn parent_virtual_path(path: &str) -> String {
    entry_parent_path(path)
}

pub(crate) fn vault_state(app: &EncryptApp) -> &'static str {
    if app.busy {
        "处理中"
    } else if app.handle.is_some() {
        "已打开"
    } else {
        "未打开"
    }
}

pub(crate) fn can_create_or_open(app: &EncryptApp) -> bool {
    !app.busy
        && app.handle.is_none()
        && !app.vault_path.trim().is_empty()
        && !app.password.is_empty()
}

pub(crate) fn entry_index_in_current_dir(
    entries: &[VaultEntry],
    current_dir: &str,
    search_query: &str,
    sort_mode: SortMode,
    path: &str,
) -> Option<usize> {
    visible_paths_in_current_dir(entries, current_dir, search_query, sort_mode)
        .iter()
        .position(|p| p == path)
}

pub(crate) fn visible_paths_in_current_dir(
    entries: &[VaultEntry],
    current_dir: &str,
    search_query: &str,
    sort_mode: SortMode,
) -> Vec<String> {
    current_dir_entries(entries, current_dir, search_query, sort_mode)
        .iter()
        .map(|e| e.virtual_path.as_str().to_string())
        .collect()
}

pub(crate) fn all_folder_paths(entries: &[VaultEntry]) -> Vec<String> {
    let mut folders: Vec<String> = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Directory)
        .map(|e| e.virtual_path.as_str().to_string())
        .collect();
    folders.sort();
    folders
}
