mod message;
mod operations;
mod state;
mod subscription;
mod update;

use iced::{window, Font, Size, Task, Theme};

use crate::{branding, formatting::load_cjk_font, view::view};

pub(crate) use message::Message;
pub(crate) use operations::OperationResult;
pub(crate) use state::{EncryptApp, SortMode};
pub(crate) use subscription::subscription;
pub(crate) use update::update;

pub(crate) fn run() -> iced::Result {
    let mut app = iced::application(branding::APP_NAME, update, view)
        .subscription(subscription)
        .exit_on_close_request(false)
        .window(window::Settings {
            size: Size::new(1280.0, 820.0),
            min_size: Some(Size::new(1040.0, 680.0)),
            icon: branding::window_icon(),
            ..Default::default()
        })
        .theme(|_| Theme::Light);

    if let Some((font_name, font_bytes)) = load_cjk_font() {
        app = app
            .font(font_bytes)
            .default_font(Font::with_name(font_name));
    } else {
        app = app.default_font(Font::with_name("Microsoft YaHei"));
    }

    app.run_with(|| (EncryptApp::default(), Task::none()))
}
