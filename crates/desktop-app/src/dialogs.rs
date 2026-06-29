use std::path::PathBuf;

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use vault_core::ImportSummary;

use crate::formatting::format_bytes;
pub(crate) async fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_file()
}

pub(crate) async fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

pub(crate) async fn confirm_cancel_operation() -> bool {
    matches!(
        MessageDialog::new()
            .set_title("确认取消当前操作")
            .set_description(
                "当前导入/导出还没有完成。\n\n确认后会在当前分块结束时停止，并清理未完成的临时文件；原文件不会被删除。\n\n要取消当前操作吗？",
            )
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

pub(crate) async fn confirm_close_while_busy() -> bool {
    matches!(
        MessageDialog::new()
            .set_title("正在处理文件")
            .set_description(
                "当前还有导入/导出任务正在进行。\n\n建议使用“取消当前操作”按钮停止任务。若继续关闭，程序会先请求取消并尽量清理临时文件，清理完成后再关闭窗口。\n\n确认退出并取消当前操作吗？",
            )
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

pub(crate) async fn confirm_cleanup_orphans(orphan_count: usize, reclaimable_bytes: u64) -> bool {
    matches!(
        MessageDialog::new()
            .set_title("确认清理无引用密文")
            .set_description(format!(
                "将删除 {} 个未被索引引用的密文文件，释放约 {} 的磁盘空间。\n\n这些文件不在保险库索引中，不会影响正常文件。\n\n确认清理吗？",
                orphan_count,
                format_bytes(reclaimable_bytes)
            ))
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

pub(crate) async fn confirm_move_entries(sources: Vec<String>, dest: String) -> bool {
    let list = sources
        .iter()
        .map(|s| format!("  {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    matches!(
        MessageDialog::new()
            .set_title("确认移动到文件夹")
            .set_description(format!(
                "将以下 {} 个条目移动到 {dest}：\n\n{list}\n\n确认移动吗？",
                sources.len()
            ))
            .set_level(MessageLevel::Info)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

pub(crate) async fn confirm_delete_entries(paths: Vec<String>, summary: ImportSummary) -> bool {
    let list = paths
        .iter()
        .map(|s| format!("  {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    matches!(
        MessageDialog::new()
            .set_title("确认从保险库删除")
            .set_description(format!(
                "将从保险库中删除以下 {} 个条目：\n\n{list}\n\n影响范围：{} 个文件，{} 个文件夹，原文件总大小 {}。\n\n保险库外面的原文件不会被删除。\n\n确认删除吗？",
                paths.len(),
                summary.files,
                summary.directories,
                format_bytes(summary.bytes)
            ))
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}
