use iced::{
    alignment,
    widget::{column, container, row, svg, text},
    Element, Length,
};

use crate::{
    app::{EncryptApp, Message},
    branding,
    browser::vault_state,
    style,
};

pub(crate) fn build_header(app: &EncryptApp) -> Element<'_, Message> {
    let state_label = vault_state(app);
    let logo = svg(svg::Handle::from_memory(
        &include_bytes!("../../../../assets/aegis-vault-mark.svg")[..],
    ))
    .width(Length::Fixed(42.0))
    .height(Length::Fixed(42.0));

    let brand_mark = container(logo)
        .width(Length::Fixed(54.0))
        .height(Length::Fixed(54.0))
        .center_x(Length::Fixed(54.0))
        .center_y(Length::Fixed(54.0))
        .style(style::brand_mark());

    let title_block = column![
        text(branding::APP_NAME).size(30).color(style::PRIMARY_DARK),
        text(branding::APP_SUBTITLE)
            .size(14)
            .color(style::TEXT_SECONDARY),
    ]
    .spacing(2);

    let status = container(
        text(format!("状态：{}", state_label))
            .size(14)
            .color(style::status_color(state_label)),
    )
    .padding([6, 10])
    .style(style::section_band());

    let content = column![
        row![brand_mark, title_block]
            .spacing(14)
            .align_y(alignment::Vertical::Center),
        status,
    ]
    .spacing(12)
    .padding(18);

    container(content)
        .width(Length::Fill)
        .style(style::header_card())
        .into()
}
