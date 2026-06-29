use iced::{
    widget::{button, column, container, progress_bar, row, text},
    Element, Length,
};
use vault_core::OperationKind;

use crate::{
    app::{EncryptApp, Message},
    formatting::{format_bytes, format_duration_seconds},
    style,
};

pub(crate) fn build_progress(app: &EncryptApp) -> Element<'_, Message> {
    if app.operation_control.is_none() {
        return column![].into();
    }

    let label = match app.progress.kind {
        OperationKind::Import => "导入加密",
        OperationKind::Export => "导出解密",
        OperationKind::Idle => "处理中",
    };

    let mut header_text = format!(
        "{}：{:.1}%（{} / {}，文件 {} / {}）",
        label,
        app.progress.percent(),
        format_bytes(app.progress.bytes_done),
        format_bytes(app.progress.bytes_total),
        app.progress.files_done,
        app.progress.files_total,
    );

    if app.progress.bytes_per_second > 0.0 {
        let speed_mb = app.progress.bytes_per_second / (1024.0 * 1024.0);
        header_text.push_str(&format!("，{:.1} MB/s", speed_mb));
    }
    if let Some(eta) = app.progress.eta_seconds {
        if eta > 0.0 {
            header_text.push_str(&format!("，剩余 {}", format_duration_seconds(eta)));
        }
    }
    if app.progress.failed_count > 0 {
        header_text.push_str(&format!("，失败 {} 个", app.progress.failed_count));
    }

    let header = text(header_text).size(15);

    let current_path = text(
        app.progress
            .current_path
            .clone()
            .unwrap_or_else(|| "准备中...".to_string()),
    )
    .size(14)
    .color(style::TEXT_SECONDARY);

    let mut content = column![
        container(header)
            .padding(10)
            .style(style::progress_header()),
        progress_bar(0.0..=100.0, app.progress.percent()).width(Length::Fill),
        current_path,
    ]
    .spacing(8);

    if app.progress.current_file_bytes_total > 0 {
        let file_pct = if app.progress.current_file_bytes_total > 0 {
            (app.progress.current_file_bytes_done as f64
                / app.progress.current_file_bytes_total as f64
                * 100.0)
                .min(100.0) as f32
        } else {
            0.0
        };
        content = content.push(
            text(format!(
                "当前文件：{} / {}（{:.1}%）",
                format_bytes(app.progress.current_file_bytes_done),
                format_bytes(app.progress.current_file_bytes_total),
                file_pct
            ))
            .size(13)
            .color(style::TEXT_SECONDARY),
        );
        content = content.push(progress_bar(0.0..=100.0, file_pct).width(Length::Fill));
    }

    if app.progress.failed_count > 0 {
        if let Some(control) = &app.operation_control {
            let report = control.report_snapshot();
            if !report.issues.is_empty() {
                content = content.push(
                    text(format!("跳过失败文件详情（{} 个）：", report.issues.len()))
                        .size(13)
                        .color(style::WARNING),
                );
                for issue in &report.issues {
                    content = content.push(
                        text(format!("  {}：{}", issue.path, issue.error))
                            .size(12)
                            .color(style::WARNING),
                    );
                }
            }
        }
    }

    let hint = text(
        "提示：推荐点击\u{201C}取消当前操作\u{201D}，系统会先弹窗确认。右上角 X 只作为误操作保护；\
         若系统绕过拦截直接关闭，原文件不会删除，残留临时文件会在下次打开保险库或导出前自动清理。",
    )
    .size(13)
    .color(style::TEXT_SECONDARY);

    content = content.push(hint);
    content = content.push(
        row![button("取消当前操作")
            .on_press(Message::RequestCancelOperation)
            .style(style::warning_button()),]
        .spacing(8),
    );

    content = content.padding(18);

    container(content)
        .width(Length::Fill)
        .style(style::card())
        .into()
}
