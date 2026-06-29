use iced::{
    widget::{button, column, container, row, text},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    formatting::format_bytes,
    settings::VaultRecord,
    style,
};

pub(crate) fn build_vault_management(app: &EncryptApp) -> Element<'_, Message> {
    let title_row = row![
        text("保险库管理").size(20).color(style::TEXT_PRIMARY),
        button("刷新")
            .on_press_maybe((!app.busy).then_some(Message::RefreshVaultRecords))
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let records = if app.vaults.is_empty() {
        column![text("暂无保险库记录。")
            .size(14)
            .color(style::TEXT_SECONDARY)]
        .spacing(6)
    } else {
        app.vaults
            .iter()
            .fold(column![].spacing(8), |list, record| {
                list.push(vault_record_view(record, app.busy, app.handle.is_none()))
            })
    };

    container(column![title_row, records].spacing(12).padding(18))
        .width(Length::Fill)
        .style(style::card())
        .into()
}

fn vault_record_view(record: &VaultRecord, busy: bool, can_select: bool) -> Element<'_, Message> {
    let status_text = if record.exists { "存在" } else { "不存在" };
    let status_color = if record.exists {
        style::SUCCESS
    } else {
        style::ERROR
    };

    let file_count = record
        .file_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "未知".to_string());
    let original_bytes = record
        .plaintext_bytes
        .map(format_bytes)
        .unwrap_or_else(|| "未知".to_string());
    let vault_size = record
        .vault_size
        .map(format_bytes)
        .unwrap_or_else(|| "未知".to_string());
    let blob_count = record
        .blob_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "未知".to_string());
    let created_at = record.created_at.as_deref().unwrap_or("未知");
    let last_opened_at = record.last_opened_at.as_deref().unwrap_or("未知");

    let content = column![
        text(record.path.clone())
            .size(14)
            .color(style::TEXT_PRIMARY)
            .width(Length::Fill),
        row![
            text("状态：").size(13).color(style::TEXT_SECONDARY),
            text(status_text).size(13).color(status_color),
        ]
        .spacing(2),
        text(format!(
            "创建：{}  |  上次打开：{}",
            created_at, last_opened_at
        ))
        .size(13)
        .color(style::TEXT_SECONDARY),
        text(format!(
            "文件数：{}  |  原文件总大小：{}  |  保险库占用：{}  |  密文文件：{}",
            file_count, original_bytes, vault_size, blob_count,
        ))
        .size(13)
        .color(style::TEXT_SECONDARY),
        row![
            button("填入")
                .on_press_maybe(
                    (!busy && can_select).then_some(Message::UseVaultRecord(record.path.clone())),
                )
                .style(style::secondary_button()),
            button("打开文件夹")
                .on_press_maybe(
                    (!busy && record.exists)
                        .then_some(Message::RevealVaultFolder(record.path.clone())),
                )
                .style(style::secondary_button()),
            button("移除")
                .on_press_maybe((!busy).then_some(Message::RemoveVaultRecord(record.path.clone())))
                .style(style::danger_outline_button()),
        ]
        .spacing(8),
    ]
    .spacing(5)
    .padding(12);

    container(content)
        .width(Length::Fill)
        .style(style::sub_card())
        .into()
}
