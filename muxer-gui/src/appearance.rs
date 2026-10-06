use iced::{
    Border, Color, Theme, color,
    theme::Palette,
    widget::{button, container},
};

pub const BACKGROUND: Color = color!(0xf6f7f4);
pub const SURFACE: Color = color!(0xffffff);
pub const SIDEBAR: Color = color!(0xf0f2ed);
pub const FOREGROUND: Color = color!(0x263c36);
pub const MUTED: Color = color!(0x75847a);
pub const PRIMARY: Color = color!(0x276551);
pub const BORDER: Color = color!(0xdfe6dd);
pub const TERMINAL: Color = color!(0x111a20);

pub fn theme() -> Theme {
    Theme::custom(
        "Solmu",
        Palette {
            background: BACKGROUND,
            text: FOREGROUND,
            primary: PRIMARY,
            success: PRIMARY,
            warning: color!(0x9b773f),
            danger: color!(0xa53535),
        },
    )
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SIDEBAR.into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

pub fn terminal(_: &Theme) -> container::Style {
    container::Style {
        background: Some(TERMINAL.into()),
        border: Border {
            color: color!(0x25323a),
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let alpha = if matches!(status, button::Status::Disabled) {
        0.55
    } else {
        1.0
    };
    button::Style {
        background: Some(PRIMARY.scale_alpha(alpha).into()),
        text_color: Color::WHITE.scale_alpha(alpha),
        border: Border {
            radius: 9.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn navigation(_: &Theme, status: button::Status, selected: bool) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: if selected {
            Some(color!(0xe0e9df).into())
        } else if hovered {
            Some(color!(0xe7ebe4).into())
        } else {
            None
        },
        text_color: if selected { PRIMARY } else { FOREGROUND },
        border: Border {
            radius: 9.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| color!(0xe7ebe4).into()),
        text_color: FOREGROUND,
        border: Border {
            radius: 9.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
