use iced::{
    border::Radius,
    widget::{button, container, text_input as text_input_widget},
    Background, Border, Color, Theme,
};

// ── Color Palette ──

pub(crate) const PRIMARY: Color = Color::from_rgb(0.086, 0.494, 0.471);
pub(crate) const PRIMARY_DARK: Color = Color::from_rgb(0.059, 0.227, 0.298);
pub(crate) const ACCENT: Color = Color::from_rgb(0.894, 0.545, 0.173);
pub(crate) const SUCCESS: Color = Color::from_rgb(0.090, 0.529, 0.329);
pub(crate) const ERROR: Color = Color::from_rgb(0.765, 0.212, 0.208);
pub(crate) const WARNING: Color = Color::from_rgb(0.788, 0.384, 0.094);
pub(crate) const INFO: Color = Color::from_rgb(0.086, 0.494, 0.471);
pub(crate) const CARD_BG: Color = Color::from_rgb(0.992, 0.992, 0.988);
pub(crate) const CARD_BORDER: Color = Color::from_rgb(0.859, 0.878, 0.878);
pub(crate) const PAGE_BG: Color = Color::from_rgb(0.949, 0.954, 0.953);
pub(crate) const TEXT_PRIMARY: Color = Color::from_rgb(0.118, 0.145, 0.161);
pub(crate) const TEXT_SECONDARY: Color = Color::from_rgb(0.408, 0.451, 0.467);
pub(crate) const MUTED_BG: Color = Color::from_rgb(0.941, 0.961, 0.957);
pub(crate) const SELECTED_BG: Color = Color::from_rgb(0.831, 0.933, 0.914);
pub(crate) const WHITE: Color = Color::from_rgb(1.0, 1.0, 1.0);

const DARKEN_HOVER: f32 = 0.12;
const LIGHTEN_PRESSED: f32 = 0.06;
const CARD_RADIUS: f32 = 8.0;
const BUTTON_RADIUS: f32 = 6.0;
const SUB_CARD_RADIUS: f32 = 6.0;

fn darken(color: Color, amount: f32) -> Color {
    Color::from_rgb(
        (color.r - amount).max(0.0),
        (color.g - amount).max(0.0),
        (color.b - amount).max(0.0),
    )
}

fn lighten(color: Color, amount: f32) -> Color {
    Color::from_rgb(
        (color.r + amount).min(1.0),
        (color.g + amount).min(1.0),
        (color.b + amount).min(1.0),
    )
}

// ── Container / Card Styles ──

pub(crate) fn page_bg() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(PAGE_BG)),
        ..Default::default()
    }
}

pub(crate) fn card() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(CARD_BG)),
        border: Border {
            color: CARD_BORDER,
            width: 1.0,
            radius: Radius::from(CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn status_bar() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(WHITE)),
        border: Border {
            color: CARD_BORDER,
            width: 1.0,
            radius: Radius::from(CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn header_card() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.976, 0.987, 0.984))),
        border: Border {
            color: Color::from_rgb(0.784, 0.847, 0.835),
            width: 1.0,
            radius: Radius::from(CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn brand_mark() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(WHITE)),
        border: Border {
            color: ACCENT,
            width: 1.0,
            radius: Radius::from(8.0),
        },
        ..Default::default()
    }
}

pub(crate) fn sub_card() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(WHITE)),
        border: Border {
            color: CARD_BORDER,
            width: 1.0,
            radius: Radius::from(SUB_CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn selected_row() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(SELECTED_BG)),
        border: Border {
            radius: Radius::from(4.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(crate) fn inset_sub_card() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(MUTED_BG)),
        border: Border {
            color: CARD_BORDER,
            width: 1.0,
            radius: Radius::from(SUB_CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn section_band() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(MUTED_BG)),
        border: Border {
            color: Color::from_rgb(0.816, 0.863, 0.855),
            width: 1.0,
            radius: Radius::from(SUB_CARD_RADIUS),
        },
        ..Default::default()
    }
}

pub(crate) fn progress_header() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.984, 0.941, 0.882))),
        border: Border {
            radius: Radius::from(6.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ── Button Styles ──

pub(crate) fn primary_button() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme, status| {
        let bg = match status {
            button::Status::Disabled => Color::from_rgb(0.6, 0.6, 0.6),
            button::Status::Hovered => darken(PRIMARY, DARKEN_HOVER),
            button::Status::Pressed => lighten(PRIMARY, LIGHTEN_PRESSED),
            button::Status::Active => PRIMARY,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: WHITE,
            border: Border {
                radius: Radius::from(BUTTON_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub(crate) fn secondary_button() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme, status| {
        let (bg, text, border_color) = match status {
            button::Status::Disabled => (
                Color::from_rgb(0.894, 0.902, 0.902),
                Color::from_rgb(0.584, 0.604, 0.608),
                Color::from_rgb(0.820, 0.839, 0.839),
            ),
            button::Status::Hovered => (
                Color::from_rgb(0.929, 0.965, 0.957),
                PRIMARY_DARK,
                Color::from_rgb(0.565, 0.729, 0.694),
            ),
            button::Status::Pressed => (
                Color::from_rgb(0.875, 0.933, 0.922),
                PRIMARY_DARK,
                Color::from_rgb(0.486, 0.663, 0.624),
            ),
            button::Status::Active => (WHITE, TEXT_PRIMARY, Color::from_rgb(0.753, 0.788, 0.792)),
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: text,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(BUTTON_RADIUS),
            },
            ..Default::default()
        }
    }
}

pub(crate) fn danger_button() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme, status| {
        let bg = match status {
            button::Status::Disabled => Color::from_rgb(0.6, 0.6, 0.6),
            button::Status::Hovered => darken(ERROR, DARKEN_HOVER),
            button::Status::Pressed => lighten(ERROR, LIGHTEN_PRESSED),
            button::Status::Active => ERROR,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: WHITE,
            border: Border {
                radius: Radius::from(BUTTON_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub(crate) fn danger_outline_button() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme, status| {
        let (bg, text, border_color) = match status {
            button::Status::Disabled => (
                Color::from_rgb(0.94, 0.94, 0.94),
                Color::from_rgb(0.55, 0.55, 0.55),
                Color::from_rgb(0.7, 0.7, 0.7),
            ),
            button::Status::Hovered => (Color::from_rgb(0.98, 0.90, 0.90), ERROR, ERROR),
            button::Status::Pressed => (Color::from_rgb(0.95, 0.85, 0.85), ERROR, ERROR),
            button::Status::Active => (WHITE, ERROR, ERROR),
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: text,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(BUTTON_RADIUS),
            },
            ..Default::default()
        }
    }
}

pub(crate) fn warning_button() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme, status| {
        let bg = match status {
            button::Status::Disabled => Color::from_rgb(0.6, 0.6, 0.6),
            button::Status::Hovered => darken(WARNING, DARKEN_HOVER),
            button::Status::Pressed => lighten(WARNING, LIGHTEN_PRESSED),
            button::Status::Active => WARNING,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: WHITE,
            border: Border {
                radius: Radius::from(BUTTON_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub(crate) fn text_input() -> impl Fn(&Theme, text_input_widget::Status) -> text_input_widget::Style
{
    |_theme, status| {
        let border_color = match status {
            text_input_widget::Status::Focused => PRIMARY,
            text_input_widget::Status::Hovered => Color::from_rgb(0.667, 0.741, 0.733),
            text_input_widget::Status::Active => Color::from_rgb(0.780, 0.808, 0.812),
            text_input_widget::Status::Disabled => Color::from_rgb(0.835, 0.847, 0.851),
        };

        text_input_widget::Style {
            background: Background::Color(WHITE),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            icon: TEXT_SECONDARY,
            placeholder: Color::from_rgb(0.596, 0.631, 0.639),
            value: TEXT_PRIMARY,
            selection: SELECTED_BG,
        }
    }
}

// ── Status Text Color ──

pub(crate) fn status_color(text: &str) -> Color {
    if text.is_empty() {
        return TEXT_SECONDARY;
    }

    let success = text.contains("已打开")
        || text.contains("已创建")
        || text.contains("已加密保存")
        || text.contains("已导出")
        || text.contains("已锁定")
        || text.contains("已从列表移除")
        || text.contains("已刷新")
        || text.contains("已取消删除")
        || text.contains("已继续")
        || text.contains("操作已取消");

    let error = text.contains("失败")
        || text.contains("错误")
        || text.contains("不正确")
        || text.contains("不存在")
        || text.contains("无法");

    let progress = text.contains("正在");

    let warning = text.contains("请先");

    if success {
        SUCCESS
    } else if error {
        ERROR
    } else if progress {
        INFO
    } else if warning {
        WARNING
    } else {
        TEXT_PRIMARY
    }
}
