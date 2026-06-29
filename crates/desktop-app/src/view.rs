mod file_operations;
mod header;
mod health;
mod progress;
mod status;
mod vault_contents;
mod vault_controls;
mod vault_management;

use iced::{
    widget::{column, container, responsive, row, scrollable},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    style,
    view::{
        file_operations::build_file_controls, header::build_header, health::build_health_panel,
        progress::build_progress, status::build_status_bar, vault_contents::build_vault_contents,
        vault_controls::build_vault_controls, vault_management::build_vault_management,
    },
};

pub(crate) fn view(app: &EncryptApp) -> Element<'_, Message> {
    let workspace = responsive(move |size| build_workspace(app, size.width < 980.0));

    let shell = column![workspace, build_status_bar(app)]
        .spacing(12)
        .padding(18)
        .width(Length::Fill)
        .height(Length::Fill);

    container(shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::page_bg())
        .into()
}

fn build_workspace(app: &EncryptApp, compact: bool) -> Element<'_, Message> {
    let left_panel = build_left_panel(app);
    let right_panel = build_right_panel(app);

    if compact {
        return column![
            container(left_panel).width(Length::Fill),
            container(right_panel).width(Length::Fill),
        ]
        .spacing(14)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    row![
        container(left_panel)
            .width(Length::FillPortion(4))
            .max_width(470.0),
        container(right_panel).width(Length::FillPortion(7)),
    ]
    .spacing(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn build_left_panel(app: &EncryptApp) -> Element<'_, Message> {
    let left_content = column![
        build_header(app),
        build_vault_controls(app),
        build_file_controls(app),
        build_vault_management(app),
    ]
    .spacing(14)
    .width(Length::Fill);

    let left_panel = scrollable(left_content)
        .width(Length::Fill)
        .height(Length::Fill);

    left_panel.into()
}

fn build_right_panel(app: &EncryptApp) -> Element<'_, Message> {
    let right_content = column![
        build_vault_contents(app),
        build_health_panel(app),
        build_progress(app)
    ]
    .spacing(14)
    .width(Length::Fill);

    let right_panel = scrollable(right_content)
        .width(Length::Fill)
        .height(Length::Fill);

    right_panel.into()
}
