use iced::{
    widget::{button, column, container, row, scrollable, text},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    formatting::format_bytes,
    style,
};

pub(crate) fn build_health_panel(app: &EncryptApp) -> Element<'_, Message> {
    if !app.showing_health {
        return column![].into();
    }

    let Some(report) = &app.health_report else {
        return column![].into();
    };

    let has_issues = !report.vault_json_exists
        || !report.vault_json_valid
        || !report.index_decryptable
        || !report.missing_blobs.is_empty()
        || !report.orphan_blobs.is_empty();

    let status_text = if has_issues {
        text("发现问题").size(18).color(style::ERROR)
    } else {
        text("正常").size(18).color(style::SUCCESS)
    };

    let config_status = format!(
        "配置：{}",
        if report.vault_json_exists && report.vault_json_valid {
            "正常"
        } else if !report.vault_json_exists {
            "缺失"
        } else {
            "无效"
        }
    );
    let config_color = if report.vault_json_exists && report.vault_json_valid {
        style::SUCCESS
    } else {
        style::ERROR
    };

    let index_status = format!(
        "索引：{}，{} 个文件",
        if report.index_decryptable {
            "可解密"
        } else {
            "损坏或不可解密"
        },
        report.total_files_in_index
    );
    let index_color = if report.index_decryptable {
        style::SUCCESS
    } else {
        style::ERROR
    };

    let mut summary = column![
        text(config_status.clone()).size(14).color(config_color),
        text(index_status).size(14).color(index_color),
    ]
    .spacing(4);

    if !report.missing_blobs.is_empty() {
        let missing_header =
            text(format!("缺失密文：{} 个", report.missing_blobs.len())).color(style::ERROR);
        summary = summary.push(missing_header);
        summary = summary.push(
            text("建议从备份恢复缺失密文；不要自动删除索引记录。")
                .size(13)
                .color(style::TEXT_SECONDARY),
        );
        let mut missing_list = column![]
            .spacing(2)
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fixed(120.0));

        for entry in &report.missing_blobs {
            missing_list = missing_list.push(
                text(format!(
                    "  {} — blob={}，{}",
                    entry.virtual_path,
                    &entry.blob_id[..12.min(entry.blob_id.len())],
                    format_bytes(entry.size)
                ))
                .size(12)
                .color(style::ERROR),
            );
        }
        let missing_scrollable = scrollable(missing_list).width(Length::Fill);
        summary = summary.push(missing_scrollable);
    }

    if !report.orphan_blobs.is_empty() {
        let orphan_header = text(format!(
            "无引用密文：{} 个，可回收 {}",
            report.orphan_blobs.len(),
            format_bytes(report.reclaimable_bytes)
        ))
        .color(style::WARNING);
        summary = summary.push(orphan_header);
    }

    let actions = row![
            button("重新检查")
                .on_press_maybe(
                    (!app.busy
                        && (app.handle.is_some()
                            || (!app.vault_path.trim().is_empty() && !app.password.is_empty())))
                    .then_some(Message::HealthCheck),
                )
                .style(style::secondary_button()),
            button(text(format!(
                "清理无引用密文（{} 个）",
                report.orphan_blobs.len()
            )))
            .on_press_maybe(
                (app.handle.is_some() && !app.busy && !report.orphan_blobs.is_empty())
                    .then_some(Message::CleanupOrphans)
            )
            .style(style::warning_button()),
            button("导出检查报告")
                .on_press_maybe((!app.busy).then_some(Message::ExportHealthReport))
                .style(style::secondary_button()),
            button("打开保险库文件夹")
                .on_press_maybe(
                    (app.handle.is_some() && !app.busy).then_some(Message::OpenVaultFolder),
                )
                .style(style::secondary_button()),
        ]
    .spacing(8);

    let header_row = row![status_text,].spacing(8);

    let content = column![header_row, summary, actions]
        .spacing(12)
        .padding(18);

    container(content)
        .width(Length::Fill)
        .style(style::card())
        .into()
}
