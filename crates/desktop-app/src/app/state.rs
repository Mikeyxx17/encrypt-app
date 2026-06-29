use iced::{keyboard, window};
use vault_core::{
    FailurePolicy, ImportConflictPolicy, OperationControl, OperationProgress, VaultEntry,
    VaultHandle, VaultHealthReport,
};

use crate::settings::{load_vault_records, VaultRecord};
#[derive(Debug)]
pub(crate) struct EncryptApp {
    pub(crate) vault_path: String,
    pub(crate) password: String,
    pub(crate) import_path: String,
    pub(crate) export_path: String,
    pub(crate) current_dir: String,
    pub(crate) selected_entries: Vec<String>,
    pub(crate) last_clicked_index: Option<usize>,
    pub(crate) last_click_time: Option<std::time::Instant>,
    pub(crate) last_clicked_path: Option<String>,
    pub(crate) search_query: String,
    pub(crate) new_folder_name: String,
    pub(crate) rename_name: String,
    pub(crate) sort_mode: SortMode,
    pub(crate) import_conflict_policy: ImportConflictPolicy,
    pub(crate) handle: Option<VaultHandle>,
    pub(crate) entries: Vec<VaultEntry>,
    pub(crate) status: String,
    pub(crate) busy: bool,
    pub(crate) operation_control: Option<OperationControl>,
    pub(crate) progress: OperationProgress,
    pub(crate) vaults: Vec<VaultRecord>,
    pub(crate) pending_close: Option<window::Id>,
    pub(crate) confirming_cancel: bool,
    pub(crate) confirming_close: bool,
    pub(crate) confirming_delete: bool,
    pub(crate) failure_policy: FailurePolicy,
    pub(crate) health_report: Option<VaultHealthReport>,
    pub(crate) showing_health: bool,
    pub(crate) confirming_cleanup: bool,
    pub(crate) showing_change_password: bool,
    pub(crate) old_password: String,
    pub(crate) new_password: String,
    pub(crate) new_password_confirm: String,
    pub(crate) move_destination: String,
    pub(crate) confirming_move: bool,
    pub(crate) current_modifiers: keyboard::Modifiers,
    pub(crate) right_click_move_source: Option<String>,
    pub(crate) showing_right_click_picker: bool,
}

impl Default for EncryptApp {
    fn default() -> Self {
        Self {
            vault_path: String::new(),
            password: String::new(),
            import_path: String::new(),
            export_path: String::new(),
            current_dir: "/".to_string(),
            selected_entries: Vec::new(),
            last_clicked_index: None,
            last_click_time: None,
            last_clicked_path: None,
            search_query: String::new(),
            new_folder_name: String::new(),
            rename_name: String::new(),
            sort_mode: SortMode::NameAsc,
            import_conflict_policy: ImportConflictPolicy::Rename,
            handle: None,
            entries: Vec::new(),
            status: "未打开保险库。".to_string(),
            busy: false,
            operation_control: None,
            progress: OperationProgress::default(),
            vaults: load_vault_records(),
            pending_close: None,
            confirming_cancel: false,
            confirming_close: false,
            confirming_delete: false,
            failure_policy: FailurePolicy::StopOnFirstError,
            health_report: None,
            showing_health: false,
            confirming_cleanup: false,
            showing_change_password: false,
            old_password: String::new(),
            new_password: String::new(),
            new_password_confirm: String::new(),
            move_destination: String::new(),
            confirming_move: false,
            current_modifiers: keyboard::Modifiers::empty(),
            right_click_move_source: None,
            showing_right_click_picker: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortMode {
    NameAsc,
    NameDesc,
    SizeDesc,
    ModifiedDesc,
}

impl SortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NameAsc => "名称 A-Z",
            Self::NameDesc => "名称 Z-A",
            Self::SizeDesc => "大小优先",
            Self::ModifiedDesc => "最近修改",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::NameAsc => Self::NameDesc,
            Self::NameDesc => Self::SizeDesc,
            Self::SizeDesc => Self::ModifiedDesc,
            Self::ModifiedDesc => Self::NameAsc,
        }
    }
}
