use iced::{
    widget::{button, column, container, row, text, text_input},
    Element, Length,
};
use vault_core::FailurePolicy;

use crate::{
    app::{EncryptApp, Message},
    style,
};

pub(crate) fn build_file_controls(app: &EncryptApp) -> Element<'_, Message> {
    let import_path_row = row![
        text_input("要加密导入的文件或文件夹", &app.import_path)
            .on_input(Message::ImportPathChanged)
            .padding(10)
            .width(Length::Fill),
        button("文件")
            .on_press_maybe((app.handle.is_some() && !app.busy).then_some(Message::PickImportFile),)
            .style(style::secondary_button()),
        button("文件夹")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy).then_some(Message::PickImportFolder),
            )
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let export_path_row = row![
        text_input(
            "导出到保险库之外的文件夹，例如 D:\\DecryptedFiles",
            &app.export_path,
        )
        .on_input(Message::ExportPathChanged)
        .padding(10)
        .width(Length::Fill),
        button("选择")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy).then_some(Message::PickExportFolder),
            )
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let actions = row![
        button(text(format!(
            "同名：{}",
            import_conflict_policy_label(app.import_conflict_policy)
        )))
        .on_press_maybe((!app.busy).then_some(Message::CycleImportConflictPolicy))
        .style(style::secondary_button()),
        button(text(format!(
            "失败：{}",
            failure_policy_label(app.failure_policy)
        )))
        .on_press_maybe((!app.busy).then_some(Message::CycleFailurePolicy))
        .style(style::secondary_button()),
        button("导入加密")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy && !app.import_path.trim().is_empty())
                    .then_some(Message::ImportPath),
            )
            .style(style::primary_button()),
        button("全部导出")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy && !app.export_path.trim().is_empty())
                    .then_some(Message::ExportAll),
            )
            .style(style::primary_button()),
    ]
    .spacing(8);

    let content = column![
        text("导入 / 导出").size(20).color(style::TEXT_PRIMARY),
        text("导入").size(15).color(style::INFO),
        import_path_row,
        text("导出").size(15).color(style::INFO),
        export_path_row,
        actions,
    ]
    .spacing(10)
    .padding(18);

    container(content)
        .width(Length::Fill)
        .style(style::card())
        .into()
}

fn import_conflict_policy_label(policy: vault_core::ImportConflictPolicy) -> &'static str {
    match policy {
        vault_core::ImportConflictPolicy::Rename => "自动改名",
        vault_core::ImportConflictPolicy::Skip => "跳过",
        vault_core::ImportConflictPolicy::Overwrite => "覆盖",
    }
}

fn failure_policy_label(policy: FailurePolicy) -> &'static str {
    match policy {
        FailurePolicy::StopOnFirstError => "遇错停止",
        FailurePolicy::SkipFailedAndContinue => "跳过继续",
    }
}
