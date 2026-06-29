use std::time::Duration;

use iced::{keyboard, time, window, Subscription};

use crate::app::{EncryptApp, Message};
pub(crate) fn subscription(app: &EncryptApp) -> Subscription<Message> {
    let mut subscriptions = vec![
        window::close_requests().map(Message::CloseRequested),
        keyboard::on_key_press(|_key, modifiers| Some(Message::ModifiersChanged(modifiers))),
        keyboard::on_key_release(|_key, modifiers| Some(Message::ModifiersChanged(modifiers))),
    ];
    if app.operation_control.is_some() {
        subscriptions.push(time::every(Duration::from_millis(200)).map(|_| Message::ProgressTick));
    }
    Subscription::batch(subscriptions)
}
