use iced::window;

pub(crate) const APP_NAME: &str = "Aegis Vault";
pub(crate) const APP_SUBTITLE: &str = "本地加密保险库";

pub(crate) fn window_icon() -> Option<window::Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let xf = x as f32 / 31.0;
            let yf = y as f32 / 31.0;
            let center_dx = xf - 0.5;
            let center_dy = yf - 0.54;
            let distance = (center_dx * center_dx + center_dy * center_dy).sqrt();
            let shield_left = 0.15 + (0.35 * (yf - 0.08).max(0.0));
            let shield_right = 0.85 - (0.35 * (yf - 0.08).max(0.0));
            let inside_shield = yf >= 0.08 && yf <= 0.95 && xf >= shield_left && xf <= shield_right;
            let inner_ring = distance >= 0.22 && distance <= 0.30;
            let keyhole =
                distance < 0.075 || (xf >= 0.45 && xf <= 0.55 && yf >= 0.58 && yf <= 0.76);
            let amber = yf >= 0.20
                && yf <= 0.31
                && xf >= 0.36 + (0.22 - yf).abs()
                && xf <= 0.64 - (0.22 - yf).abs();

            let (r, g, b, a) = if !inside_shield {
                (0, 0, 0, 0)
            } else if amber {
                (228, 139, 44, 255)
            } else if keyhole || inner_ring {
                (15, 36, 51, 255)
            } else if distance < 0.36 {
                (240, 248, 252, 255)
            } else if xf < 0.5 {
                (22, 126, 120, 255)
            } else {
                (15, 36, 51, 255)
            };

            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    window::icon::from_rgba(rgba, SIZE, SIZE).ok()
}
