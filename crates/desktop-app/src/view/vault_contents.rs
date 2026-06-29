use iced::{
    widget::{
        button, column, container, mouse_area, row, scrollable, text,
        text::{Shaping, Wrapping},
        text_input,
    },
    Element, Length,
};
use vault_core::{EntryKind, VaultEntry};

use crate::{
    app::{EncryptApp, Message},
    browser::{
        all_folder_paths, can_export_selected, current_dir_entries, entry_kind_label,
        entry_modified_label, entry_name, entry_size_label, is_entry_selected, selected_entries,
        vault_totals,
    },
    formatting::format_bytes,
    style,
};

const TYPE_COLUMN_WIDTH: f32 = 58.0;
const SIZE_COLUMN_WIDTH: f32 = 82.0;
const MODIFIED_COLUMN_WIDTH: f32 = 132.0;
const ACTION_COLUMN_WIDTH: f32 = 116.0;

pub(crate) fn build_vault_contents(app: &EncryptApp) -> Element<'_, Message> {
    let (file_count, directory_count, total_bytes) = vault_totals(&app.entries);
    let current_entries = current_dir_entries(
        &app.entries,
        &app.current_dir,
        &app.search_query,
        app.sort_mode,
    );

    let title_row = row![
        text("保险库内容").size(20).color(style::TEXT_PRIMARY),
        button("返回上级")
            .on_press_maybe((!app.busy && app.current_dir != "/").then_some(Message::NavigateUp))
            .style(style::secondary_button()),
        button(text(format!("排序：{}", app.sort_mode.label())))
            .on_press_maybe((!app.busy).then_some(Message::CycleSortMode))
            .style(style::secondary_button()),
        button("检查保险库")
            .on_press_maybe(
                (!app.busy
                    && (app.handle.is_some()
                        || (!app.vault_path.trim().is_empty() && !app.password.is_empty())))
                .then_some(Message::HealthCheck),
            )
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let search_row = row![
        text_input("搜索当前文件夹", &app.search_query)
            .on_input(Message::SearchChanged)
            .padding(10)
            .style(style::text_input())
            .width(Length::Fill),
        text_input("新文件夹名称", &app.new_folder_name)
            .on_input(Message::NewFolderNameChanged)
            .padding(10)
            .style(style::text_input())
            .width(Length::FillPortion(1)),
        button("新建文件夹")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy && !app.new_folder_name.trim().is_empty())
                    .then_some(Message::CreateFolder),
            )
            .style(style::primary_button()),
    ]
    .spacing(8);

    let mut entries = column![
        title_row,
        search_row,
        text(format!("当前位置：{}", app.current_dir))
            .size(14)
            .color(style::TEXT_SECONDARY),
        text(format!(
            "总计：{} 个文件，{} 个文件夹，原文件总大小 {}",
            file_count,
            directory_count,
            format_bytes(total_bytes),
        ))
        .size(13)
        .color(style::TEXT_SECONDARY),
    ]
    .spacing(8);

    if app.handle.is_none() {
        entries = entries.push(
            text("打开或创建保险库后，这里会显示文件和文件夹。")
                .size(14)
                .color(style::TEXT_SECONDARY),
        );
    } else if current_entries.is_empty() {
        entries = entries.push(
            text("当前文件夹为空。")
                .size(14)
                .color(style::TEXT_SECONDARY),
        );
    } else {
        entries = entries.push(table_header());
        for entry in current_entries {
            entries = entries.push(vault_entry_row(entry, app));
        }
    }

    let mut content = column![entries, build_selected_panel(app)].spacing(14);

    // Right-click move picker
    if app.showing_right_click_picker {
        if let Some(ref source) = app.right_click_move_source {
            let folders = all_folder_paths(&app.entries);
            let mut picker_col = column![
                text(format!("右键移动：{}", source))
                    .size(14)
                    .color(style::TEXT_PRIMARY),
                button("取消")
                    .on_press(Message::CancelRightClickMove)
                    .style(style::secondary_button()),
            ]
            .spacing(6);

            let folder_list = {
                let mut list = column![].spacing(2);
                let source_prefix = format!("{}/", source);
                let folders_owned: Vec<String> = folders
                    .iter()
                    .filter(|f| *f != source && !f.starts_with(&source_prefix))
                    .cloned()
                    .collect();
                for folder in &folders_owned {
                    let s = source.clone();
                    let f = folder.clone();
                    list = list.push(
                        button(text(f.clone()).size(13))
                            .on_press(Message::PickedRightClickMoveDest(s, f))
                            .style(style::secondary_button())
                            .width(Length::Fill),
                    );
                }
                list
            };

            picker_col = picker_col.push(
                scrollable(folder_list)
                    .height(Length::Fixed(200.0))
                    .width(Length::Fill),
            );

            content =
                content.push(container(picker_col.spacing(4).padding(12)).style(style::sub_card()));
        }
    }

    container(content.padding(18))
        .width(Length::Fill)
        .style(style::card())
        .into()
}

fn build_selected_panel(app: &EncryptApp) -> Element<'_, Message> {
    let selected = selected_entries(app);
    if selected.is_empty() {
        let content = column![
            text("未选中条目").size(18).color(style::TEXT_SECONDARY),
            text("选择文件或文件夹后，可以单独导出或从保险库删除。")
                .size(14)
                .color(style::TEXT_SECONDARY),
        ]
        .spacing(6)
        .padding(14);

        return container(content)
            .width(Length::Fill)
            .style(style::inset_sub_card())
            .into();
    }

    let count = selected.len();
    let summary = if count == 1 {
        let entry = selected[0];
        column![
            text(format!("路径：{}", entry.virtual_path))
                .size(14)
                .color(style::TEXT_SECONDARY)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
            text(format!("类型：{}", entry_kind_label(entry.kind)))
                .size(14)
                .color(style::TEXT_SECONDARY),
            text(format!("大小：{}", entry_size_label(entry)))
                .size(14)
                .color(style::TEXT_SECONDARY),
            text(format!("修改时间：{}", entry_modified_label(entry)))
                .size(14)
                .color(style::TEXT_SECONDARY),
        ]
        .spacing(4)
    } else {
        let mut paths_text = String::new();
        for s in &selected {
            paths_text.push_str(&format!("  {}\n", s.virtual_path));
        }
        let trimmed = paths_text.trim_end().to_string();
        column![
            text(format!("已选中 {} 个条目", count))
                .size(14)
                .color(style::TEXT_PRIMARY),
            text(trimmed)
                .size(12)
                .color(style::TEXT_SECONDARY)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(4)
    };

    // Rename row: only shown when exactly 1 selected
    let rename_row = if count == 1 {
        Some(
            row![
                text_input("新名称", &app.rename_name)
                    .on_input(Message::RenameNameChanged)
                    .padding(10)
                    .style(style::text_input())
                    .width(Length::Fill),
                button("重命名")
                    .on_press_maybe(
                        (app.handle.is_some() && !app.busy && !app.rename_name.trim().is_empty())
                            .then_some(Message::RenameSelected),
                    )
                    .style(style::secondary_button()),
            ]
            .spacing(8),
        )
    } else {
        None
    };

    let export_enabled = can_export_selected(app);
    let mut export_hint = None;
    if !export_enabled {
        if app.busy {
            export_hint = Some("保险库正在处理中，请等待完成。");
        } else if app.export_path.trim().is_empty() {
            export_hint = Some("请先在左侧「导入 / 导出」区域设置导出目标文件夹，再导出选中条目。");
        } else if app.handle.is_none() {
            export_hint = Some("请先打开保险库。");
        }
    }

    let move_toggle = if app.confirming_move {
        row![
            text_input("目标文件夹，例如 /Documents", &app.move_destination)
                .on_input(Message::MoveDestinationChanged)
                .padding(10)
                .style(style::text_input())
                .width(Length::Fill),
            button("用当前文件夹")
                .on_press(Message::MoveDestinationChanged(app.current_dir.clone()))
                .style(style::secondary_button()),
            button("移动")
                .on_press_maybe(
                    (app.handle.is_some() && !app.busy && !app.move_destination.trim().is_empty())
                        .then_some(Message::RequestMoveSelected),
                )
                .style(style::primary_button()),
            button("取消")
                .on_press(Message::CancelMove)
                .style(style::secondary_button()),
        ]
        .spacing(8)
    } else {
        row![button("移动到...")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy)
                    .then_some(Message::MoveDestinationChanged(String::new())),
            )
            .style(style::secondary_button()),]
        .spacing(8)
    };

    let actions = row![
        button("导出选中")
            .on_press_maybe(export_enabled.then_some(Message::ExportSelected))
            .style(style::primary_button()),
        button("删除选中")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy).then_some(Message::RequestDeleteSelected),
            )
            .style(style::danger_button()),
    ]
    .spacing(8);

    let mut col = column![
        text("已选中").size(18).color(style::TEXT_PRIMARY),
        summary,
        move_toggle,
        actions,
    ];

    if let Some(rename) = rename_row {
        col = col.push(rename);
    }

    if let Some(hint) = export_hint {
        col = col.push(text(hint).size(13).color(style::WARNING));
    }

    let content = col.spacing(8).padding(14);

    container(content)
        .width(Length::Fill)
        .style(style::inset_sub_card())
        .into()
}

fn table_header<'a>() -> Element<'a, Message> {
    row![
        text("类型")
            .size(13)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fixed(TYPE_COLUMN_WIDTH)),
        text("名称")
            .size(13)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fill),
        text("大小")
            .size(13)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fixed(SIZE_COLUMN_WIDTH)),
        text("修改时间")
            .size(13)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fixed(MODIFIED_COLUMN_WIDTH)),
        text("操作")
            .size(13)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fixed(ACTION_COLUMN_WIDTH)),
    ]
    .spacing(8)
    .padding(8)
    .into()
}

fn vault_entry_row<'a>(entry: &'a VaultEntry, app: &EncryptApp) -> Element<'a, Message> {
    let path = entry.virtual_path.as_str().to_string();
    let is_selected = is_entry_selected(app, entry.virtual_path.as_str());
    let marker = if is_selected { "已选" } else { "" };

    let select_label = text(if is_selected { "✓ 已选" } else { "" })
        .size(13)
        .color(if is_selected {
            style::SUCCESS
        } else {
            style::TEXT_SECONDARY
        });
    let mut actions: iced::widget::Row<'_, Message> = row![select_label].spacing(8);

    if entry.kind == EntryKind::Directory {
        actions = actions.push(
            button("打开")
                .on_press_maybe((!app.busy).then_some(Message::NavigateTo(path.clone())))
                .padding(4)
                .style(style::primary_button()),
        );
    }

    let content = column![
        row![
            text(format!("{} {}", entry_kind_label(entry.kind), marker))
                .width(Length::Fixed(TYPE_COLUMN_WIDTH)),
            text(crate::formatting::truncate_middle(&entry_name(entry), 48))
                .width(Length::Fill)
                .shaping(Shaping::Advanced)
                .wrapping(Wrapping::WordOrGlyph),
            text(entry_size_label(entry)).width(Length::Fixed(SIZE_COLUMN_WIDTH)),
            text(entry_modified_label(entry)).width(Length::Fixed(MODIFIED_COLUMN_WIDTH)),
            actions.width(Length::Fixed(ACTION_COLUMN_WIDTH)),
        ]
        .spacing(8),
        text(entry.virtual_path.as_str())
            .size(12)
            .color(style::TEXT_SECONDARY)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(2)
    .padding(8);

    let styled: Element<'_, Message> = if is_selected {
        container(content)
            .width(Length::Fill)
            .style(style::selected_row())
            .into()
    } else {
        container(content).width(Length::Fill).into()
    };

    mouse_area(styled)
        .on_press(Message::SelectEntry(path.clone()))
        .on_right_press(Message::RightClickEntry(path))
        .into()
}
