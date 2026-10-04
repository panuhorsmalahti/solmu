use iced::{Theme, color, theme::Palette};

pub fn theme() -> Theme {
    Theme::custom(
        "Solmu",
        Palette {
            background: color!(0xfafbf8),
            text: color!(0x263c36),
            primary: color!(0x276551),
            success: color!(0x276551),
            warning: color!(0x9b773f),
            danger: color!(0xa53535),
        },
    )
}
