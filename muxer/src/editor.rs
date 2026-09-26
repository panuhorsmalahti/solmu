use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Single-line fields own their cursor and cut buffer, independently per client.
#[derive(Clone, Default)]
pub struct Editor {
    pub text: String,
    cursor: usize,
    cut: String,
}
impl Editor {
    pub fn new(text: String) -> Self {
        Self {
            cursor: text.len(),
            text,
            cut: String::new(),
        }
    }
    fn left(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn right(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |g| self.cursor + g.len())
    }
    fn word_left(&self) -> usize {
        let mut cursor = self.cursor;
        let mut word = false;
        for (i, g) in self.text[..self.cursor].grapheme_indices(true).rev() {
            if g.chars().all(char::is_whitespace) {
                if word {
                    break;
                }
            } else {
                word = true;
            }
            cursor = i;
        }
        cursor
    }
    fn word_right(&self) -> usize {
        let mut cursor = self.cursor;
        let mut word = false;
        for (i, g) in self.text[self.cursor..].grapheme_indices(true) {
            if g.chars().all(char::is_whitespace) {
                if word {
                    break;
                }
            } else {
                word = true;
            }
            cursor = self.cursor + i + g.len();
        }
        cursor
    }
    fn remove(&mut self, start: usize, end: usize, cut: bool) {
        if cut {
            self.cut = self.text[start..end].to_owned();
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.normalize_cursor();
    }
    pub fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        // Bound fields independently of the authenticated transport limit.
        if self.text.len() + text.len() > 4096 {
            return;
        }
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
        // Pasting a combining character can merge with the next grapheme.
        self.normalize_cursor();
    }
    fn normalize_cursor(&mut self) {
        if !self
            .text
            .grapheme_indices(true)
            .any(|(i, _)| i == self.cursor)
        {
            self.cursor = self
                .text
                .grapheme_indices(true)
                .find(|(i, _)| *i > self.cursor)
                .map_or(self.text.len(), |(i, _)| i);
        }
    }
    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        // Windows layouts may report AltGr characters (including path separators)
        // with Ctrl+Alt. They are text, rather than field editing shortcuts.
        if ctrl
            && alt
            && let KeyCode::Char(ch) = key.code
        {
            self.insert(&ch.to_string());
            return;
        }
        match key.code {
            KeyCode::Left if ctrl || alt => self.cursor = self.word_left(),
            KeyCode::Right if ctrl || alt => self.cursor = self.word_right(),
            KeyCode::Left => self.cursor = self.left(),
            KeyCode::Right => self.cursor = self.right(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace if ctrl || alt => self.remove(self.word_left(), self.cursor, true),
            KeyCode::Backspace => self.remove(self.left(), self.cursor, false),
            KeyCode::Delete => self.remove(self.cursor, self.right(), false),
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.text.len(),
            KeyCode::Char('b') if alt => self.cursor = self.word_left(),
            KeyCode::Char('f') if alt => self.cursor = self.word_right(),
            KeyCode::Char('b') if ctrl => self.cursor = self.left(),
            KeyCode::Char('f') if ctrl => self.cursor = self.right(),
            KeyCode::Char('h') if ctrl => self.remove(self.left(), self.cursor, false),
            KeyCode::Char('d') if ctrl => self.remove(self.cursor, self.right(), false),
            KeyCode::Char('d') if alt => self.remove(self.cursor, self.word_right(), true),
            KeyCode::Char('u') if ctrl => self.remove(0, self.cursor, true),
            KeyCode::Char('k') if ctrl => self.remove(self.cursor, self.text.len(), true),
            KeyCode::Char('w') if ctrl => self.remove(self.word_left(), self.cursor, true),
            KeyCode::Char('y') if ctrl => self.insert(&self.cut.clone()),
            KeyCode::Char(ch) if !ctrl || alt => self.insert(&ch.to_string()),
            _ => {}
        }
    }
    /// Keep the insertion point visible, respecting terminal cell widths.
    pub fn visible(&self, width: u16) -> (String, u16) {
        let width = usize::from(width.saturating_sub(1));
        let mut start = self.cursor;
        let mut cursor = 0;
        for (i, g) in self.text[..self.cursor].grapheme_indices(true).rev() {
            let size = UnicodeWidthStr::width(g);
            if cursor + size > width {
                break;
            }
            start = i;
            cursor += size;
        }
        let mut visible = String::new();
        let mut cells = 0;
        for g in self.text[start..].graphemes(true) {
            let size = UnicodeWidthStr::width(g);
            if cells + size > width {
                break;
            }
            visible.push_str(g);
            cells += size;
        }
        (visible, cursor as u16)
    }
}
