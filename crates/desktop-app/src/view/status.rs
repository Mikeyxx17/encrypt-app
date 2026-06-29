use iced::{
    widget::{container, text},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    style,
};

pub(crate) fn build_status_bar(app: &EncryptApp) -> Element<'_, Message> {
    container(
        text(&app.status)
            .size(14)
            .color(style::status_color(&app.status)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(48.0))
    .padding(12)
    .style(style::card())
    .into()
}
