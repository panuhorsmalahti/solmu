use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn encode(key: KeyEvent) -> Vec<u8> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let arrow = |code| {
        if modifier == 1 {
            format!("\x1b[{code}")
        } else {
            format!("\x1b[1;{modifier}{code}")
        }
    };
    let text = match key.code {
        // Preserve composed AltGr symbols; legacy Ctrl+Alt letter chords can
        // be represented as Escape followed by their control byte.
        KeyCode::Char(ch) if ctrl && ch.is_ascii() && (!alt || ch.is_ascii_alphabetic()) => {
            let value = ch.to_ascii_uppercase() as u8;
            if (b'@'..=b'_').contains(&value) {
                String::from_utf8(vec![value & 0x1f]).unwrap_or_default()
            } else {
                return Vec::new();
            }
        }
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Enter => "\r".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Up => arrow('A'),
        KeyCode::Down => arrow('B'),
        KeyCode::Right => arrow('C'),
        KeyCode::Left => arrow('D'),
        KeyCode::Home => arrow('H'),
        KeyCode::End => arrow('F'),
        KeyCode::PageUp => "\x1b[5~".into(),
        KeyCode::PageDown => "\x1b[6~".into(),
        KeyCode::Delete => "\x1b[3~".into(),
        KeyCode::Insert => "\x1b[2~".into(),
        KeyCode::F(n @ 1..=4) => {
            let code = char::from(b'P' + n - 1);
            if modifier == 1 {
                format!("\x1bO{code}")
            } else {
                arrow(code)
            }
        }
        KeyCode::F(n @ 5..=24) => {
            let code = [
                15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34, 42, 43, 44, 45,
            ][usize::from(n - 5)];
            if modifier == 1 {
                format!("\x1b[{code}~")
            } else {
                format!("\x1b[{code};{modifier}~")
            }
        }
        _ => return Vec::new(),
    };
    if alt
        && matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace
        )
    {
        format!("\x1b{text}").into_bytes()
    } else {
        text.into_bytes()
    }
}
