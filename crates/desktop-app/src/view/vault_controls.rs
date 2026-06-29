use iced::{
    widget::{button, column, container, row, text, text_input},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    browser::can_create_or_open,
    style,
};

pub(crate) fn build_vault_controls(app: &EncryptApp) -> Element<'_, Message> {
    let path_row = row![
        text_input("保险库文件夹，例如 D:\\Vaults\\MyVault", &app.vault_path)
            .on_input(Message::VaultPathChanged)
            .padding(10)
            .style(style::text_input())
            .width(Length::Fill),
        button("选择")
            .on_press_maybe((!app.busy).then_some(Message::PickVaultFolder))
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let password_input = text_input("Password", &app.password)
        .secure(true)
        .on_input(Message::PasswordChanged)
        .padding(10)
        .style(style::text_input())
        .width(Length::Fill);

    let primary_actions = row![
        button("新建保险库")
            .on_press_maybe(can_create_or_open(app).then_some(Message::CreateVault))
            .style(style::primary_button()),
        button("打开保险库")
            .on_press_maybe(can_create_or_open(app).then_some(Message::OpenVault))
            .style(style::primary_button()),
        button("健康检查")
            .on_press_maybe(
                (!app.busy
                    && (app.handle.is_some()
                        || (!app.vault_path.trim().is_empty() && !app.password.is_empty())))
                .then_some(Message::HealthCheck),
            )
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let secondary_actions = row![
        button("锁定")
            .on_press_maybe(app.handle.is_some().then_some(Message::LockVault))
            .style(style::secondary_button()),
        button("修改密码")
            .on_press_maybe(
                (app.handle.is_some() && !app.busy && !app.showing_change_password)
                    .then_some(Message::ShowChangePassword),
            )
            .style(style::secondary_button()),
    ]
    .spacing(8);

    let mut content = column![
        text("保险库").size(20).color(style::TEXT_PRIMARY),
        path_row,
        password_input,
        primary_actions,
        secondary_actions,
    ]
    .spacing(12)
    .padding(18);

    if app.showing_change_password {
        let change_pw_section = column![
            text("修改密码").size(16).color(style::TEXT_PRIMARY),
            text_input("旧密码", &app.old_password)
                .secure(true)
                .on_input(Message::OldPasswordChanged)
                .padding(10)
                .style(style::text_input())
                .width(Length::Fill),
            text_input("新密码", &app.new_password)
                .secure(true)
                .on_input(Message::NewPasswordChanged)
                .padding(10)
                .style(style::text_input())
                .width(Length::Fill),
            text_input("确认新密码", &app.new_password_confirm)
                .secure(true)
                .on_input(Message::NewPasswordConfirmChanged)
                .padding(10)
                .style(style::text_input())
                .width(Length::Fill),
            row![
                button("确认修改")
                    .on_press_maybe(
                        (!app.busy
                            && !app.old_password.is_empty()
                            && !app.new_password.is_empty()
                            && !app.new_password_confirm.is_empty())
                        .then_some(Message::ChangePassword),
                    )
                    .style(style::primary_button()),
                button("取消")
                    .on_press_maybe((!app.busy).then_some(Message::HideChangePassword))
                    .style(style::secondary_button()),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .padding(14);

        content = content.push(
            container(change_pw_section)
                .width(Length::Fill)
                .style(style::inset_sub_card()),
        );
    }

    container(content)
        .width(Length::Fill)
        .style(style::card())
        .into()
}
