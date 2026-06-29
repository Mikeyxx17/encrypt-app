#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod app;
mod branding;
mod browser;
mod dialogs;
mod formatting;
mod settings;
mod style;
mod view;

fn main() -> iced::Result {
    app::run()
}
