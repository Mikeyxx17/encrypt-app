use iced::{
    widget::{column, container, text},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    browser::vault_state,
    style,
};

pub(crate) fn build_header(app: &EncryptApp) -> Element<'_, Message> {
    let title = text("Encrypt App").size(32).color(style::PRIMARY);
    let state_label = vault_state(app);
    let status = text(format!("状态：{}", state_label))
        .size(16)
        .color(style::status_color(state_label));

    container(column![title, status].spacing(6).padding(18))
        .width(Length::Fill)
        .style(style::card())
        .into()
}
