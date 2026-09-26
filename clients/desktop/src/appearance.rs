use iced::{
    Border, Color, Theme, color,
    theme::Palette,
    widget::{button, container, text_input},
};

pub const BACKGROUND: Color = color!(0xfafbf8);
pub const FOREGROUND: Color = color!(0x263c36);
pub const PRIMARY: Color = color!(0x276551);
pub const MUTED: Color = color!(0x75847a);
pub const BORDER: Color = color!(0xdfe6dd);
pub const DANGER: Color = color!(0xa53535);

pub fn theme() -> Theme {
    Theme::custom(
        "Solmu",
        Palette {
            background: BACKGROUND,
            text: FOREGROUND,
            primary: PRIMARY,
            success: PRIMARY,
            warning: color!(0x9b773f),
            danger: DANGER,
        },
    )
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(color!(0xf0f3ec).into()),
        text_color: Some(FOREGROUND),
        border: Border {
            color: BORDER,
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn message(_: &Theme, user: bool) -> container::Style {
    if user {
        container::Style {
            background: Some(color!(0xf1f4ed).into()),
            border: Border {
                color: color!(0xe4e9e0),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    } else {
        container::Style::default()
    }
}

pub fn thread(_: &Theme, status: button::Status, selected: bool) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: if selected {
            Some(color!(0xe1e9dc).into())
        } else if hovered {
            Some(color!(0xe6ece1).into())
        } else {
            None
        },
        text_color: if selected { PRIMARY } else { color!(0x65786b) },
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    colored(theme, status, PRIMARY)
}
pub fn danger(theme: &Theme, status: button::Status) -> button::Style {
    colored(theme, status, DANGER)
}
fn colored(_: &Theme, status: button::Status, color: Color) -> button::Style {
    let alpha = match status {
        button::Status::Disabled => 0.5,
        button::Status::Hovered => 0.9,
        _ => 1.0,
    };
    button::Style {
        background: Some(color.scale_alpha(alpha).into()),
        text_color: Color::WHITE.scale_alpha(alpha),
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        text_color: FOREGROUND,
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| color!(0xedf1ea).into()),
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Color::WHITE.into(),
        border: Border {
            color: if matches!(status, text_input::Status::Focused { .. }) {
                PRIMARY
            } else {
                BORDER
            },
            width: 1.0,
            radius: 10.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: FOREGROUND,
        selection: color!(0xe1e9dc),
    }
}
