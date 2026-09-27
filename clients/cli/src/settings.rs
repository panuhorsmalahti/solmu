use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Stylize},
    widgets::{Block, Paragraph, Wrap},
};
use solmu_client::{Api, ModelCatalog, Profile, SkillCatalog};
use tokio::sync::mpsc;

pub enum Page {
    Skills {
        scroll: u16,
    },
    Profile {
        draft: String,
        original: String,
        model: String,
        original_model: String,
        backend_default: Option<String>,
        edited_at: String,
        model_focus: bool,
        cursor: usize,
        busy: bool,
        pending_refresh: bool,
        notice: String,
    },
    Models {
        catalog: Option<ModelCatalog>,
        selected: usize,
        current: Option<String>,
        notice: String,
    },
}
pub enum Event {
    Loaded(Result<Profile, String>),
    Saved(Result<Profile, String>),
    Models(Result<ModelCatalog, String>),
}
pub enum Action {
    None,
    Close,
    Save(String, Option<String>),
    Model(Option<String>),
}

pub fn load(api: Api, sender: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let _ = sender.send(Event::Loaded(api.profile().await));
    });
}
pub fn save(api: Api, text: String, model: Option<String>, sender: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let _ = sender.send(Event::Saved(
            api.save_profile(&text, model.as_deref()).await,
        ));
    });
}
pub fn models(api: Api, sender: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let _ = sender.send(Event::Models(api.models().await));
    });
}

impl Page {
    pub fn profile() -> Self {
        Self::Profile {
            draft: String::new(),
            original: String::new(),
            model: String::new(),
            original_model: String::new(),
            backend_default: None,
            edited_at: String::new(),
            model_focus: false,
            cursor: 0,
            busy: true,
            pending_refresh: false,
            notice: "Loading profile…".into(),
        }
    }
    pub fn models(current: Option<String>) -> Self {
        Self::Models {
            catalog: None,
            selected: 0,
            current,
            notice: "Loading models…".into(),
        }
    }
    pub fn refresh(&mut self) -> bool {
        if let Self::Profile {
            draft,
            original,
            model,
            original_model,
            busy,
            pending_refresh,
            notice,
            ..
        } = self
        {
            if *busy {
                *pending_refresh = true;
                return false;
            }
            if draft != original || model != original_model {
                *notice = "Profile changed elsewhere. Your draft is unchanged.".into();
                return false;
            }
            *busy = true;
            return true;
        }
        false
    }
    pub fn refresh_pending(&mut self) -> bool {
        if let Self::Profile {
            busy: false,
            pending_refresh,
            ..
        } = self
            && std::mem::take(pending_refresh)
        {
            return self.refresh();
        }
        false
    }
    pub fn update(&mut self, event: Event) {
        let saved = matches!(&event, Event::Saved(_));
        match (self, event) {
            (
                Self::Profile {
                    draft,
                    original,
                    model,
                    original_model,
                    backend_default,
                    edited_at,
                    cursor,
                    busy,
                    notice,
                    ..
                },
                Event::Loaded(result) | Event::Saved(result),
            ) => {
                *busy = false;
                match result {
                    Ok(profile) => {
                        let unchanged = *original == profile.system_prompt
                            && *original_model == profile.model.as_deref().unwrap_or_default();
                        let keep_saved = !saved && unchanged && notice.starts_with("Profile saved");
                        *draft = profile.system_prompt;
                        *original = draft.clone();
                        *model = profile.model.unwrap_or_default();
                        *original_model = model.clone();
                        *backend_default = profile.backend_default_model;
                        *edited_at = profile.edited_at;
                        *cursor = draft.len();
                        if !keep_saved {
                            *notice = if saved {
                                "Profile saved · edits apply to subsequent replies"
                            } else {
                                "Profile ready"
                            }
                            .into();
                        }
                    }
                    Err(error) => *notice = error,
                }
            }
            (
                Self::Models {
                    catalog,
                    selected,
                    current,
                    notice,
                },
                Event::Models(result),
            ) => match result {
                Ok(value) => {
                    *selected = current
                        .as_ref()
                        .and_then(|id| value.models.iter().position(|model| model.id == *id))
                        .map_or(0, |index| index + 1);
                    *notice = format!(
                        "{} · changes apply to the next reply",
                        value
                            .provider
                            .as_deref()
                            .unwrap_or("No provider configured")
                    );
                    *catalog = Some(value);
                }
                Err(error) => *notice = error,
            },
            _ => {}
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Esc {
            return Action::Close;
        }
        match self {
            Self::Skills { scroll } => match key.code {
                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(8),
                KeyCode::PageDown => *scroll = scroll.saturating_add(8),
                _ => {}
            },
            Self::Profile {
                draft,
                cursor,
                busy,
                notice,
                model,
                model_focus,
                ..
            } => {
                if *busy {
                    return Action::None;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('s') => {
                            if draft.trim().is_empty() {
                                *notice = "System prompt must contain text".into();
                            } else {
                                *busy = true;
                                return Action::Save(
                                    draft.clone(),
                                    (!model.trim().is_empty()).then(|| model.trim().to_owned()),
                                );
                            }
                        }
                        KeyCode::Char('u') => {
                            if *model_focus {
                                model.clear();
                            } else {
                                draft.clear();
                                *cursor = 0;
                            }
                        }
                        _ => {}
                    }
                    return Action::None;
                }
                if key.code == KeyCode::Tab {
                    *model_focus = !*model_focus;
                    return Action::None;
                }
                if *model_focus {
                    match key.code {
                        KeyCode::Char(c) => model.push(c),
                        KeyCode::Backspace => {
                            model.pop();
                        }
                        _ => {}
                    }
                    return Action::None;
                }
                match key.code {
                    KeyCode::Char(c) => {
                        draft.insert(*cursor, c);
                        *cursor += c.len_utf8();
                    }
                    KeyCode::Enter => {
                        draft.insert(*cursor, '\n');
                        *cursor += 1;
                    }
                    KeyCode::Backspace if *cursor > 0 => {
                        let previous = draft[..*cursor].char_indices().next_back().unwrap().0;
                        draft.drain(previous..*cursor);
                        *cursor = previous;
                    }
                    KeyCode::Delete if *cursor < draft.len() => {
                        let length = draft[*cursor..].chars().next().unwrap().len_utf8();
                        draft.drain(*cursor..*cursor + length);
                    }
                    KeyCode::Left if *cursor > 0 => {
                        *cursor = draft[..*cursor].char_indices().next_back().unwrap().0
                    }
                    KeyCode::Right if *cursor < draft.len() => {
                        *cursor += draft[*cursor..].chars().next().unwrap().len_utf8()
                    }
                    KeyCode::Home => {
                        *cursor = draft[..*cursor].rfind('\n').map_or(0, |index| index + 1)
                    }
                    KeyCode::End => {
                        *cursor = draft[*cursor..]
                            .find('\n')
                            .map_or(draft.len(), |index| *cursor + index)
                    }
                    KeyCode::Up | KeyCode::Down => {
                        let start = draft[..*cursor].rfind('\n').map_or(0, |index| index + 1);
                        let column = draft[start..*cursor].chars().count();
                        let target = if key.code == KeyCode::Up && start > 0 {
                            Some(draft[..start - 1].rfind('\n').map_or(0, |index| index + 1))
                        } else if key.code == KeyCode::Down {
                            draft[*cursor..].find('\n').map(|index| *cursor + index + 1)
                        } else {
                            None
                        };
                        if let Some(start) = target {
                            let line = draft[start..].split('\n').next().unwrap_or_default();
                            *cursor = start
                                + line
                                    .char_indices()
                                    .nth(column)
                                    .map_or(line.len(), |(index, _)| index);
                        }
                    }
                    _ => {}
                }
            }
            Self::Models {
                catalog: Some(catalog),
                selected,
                ..
            } => match key.code {
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Down => *selected = (*selected + 1).min(catalog.models.len()),
                KeyCode::Enter => {
                    return Action::Model(if *selected == 0 {
                        None
                    } else {
                        Some(catalog.models[*selected - 1].id.clone())
                    });
                }
                _ => {}
            },
            _ => {}
        }
        Action::None
    }
    pub fn draw(&self, frame: &mut Frame<'_>, skills: &SkillCatalog) {
        let [header, body, notice, footer] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(3),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .areas(frame.area());
        match self {
            Self::Skills { scroll } => {
                frame.render_widget(
                    Paragraph::new(
                        " SOLMU / SKILLS\n\nWorkspace skills · discovered automatically",
                    )
                    .green(),
                    header,
                );
                frame.render_widget(
                    Paragraph::new(skills.text())
                        .wrap(Wrap { trim: false })
                        .scroll((*scroll, 0)),
                    body,
                );
                frame.render_widget(
                    Paragraph::new(skills.directory.as_str())
                        .wrap(Wrap { trim: false })
                        .dark_gray(),
                    notice,
                );
                frame.render_widget(Paragraph::new("Esc back").dark_gray(), footer);
            }
            Self::Profile {
                draft,
                cursor,
                busy,
                notice: status,
                model,
                model_focus,
                backend_default,
                edited_at,
                ..
            } => {
                frame.render_widget(
                    Paragraph::new(
                        " SOLMU / PROFILE\n\nSystem prompt · shared across all threads and clients",
                    )
                    .green(),
                    header,
                );
                let inner = Block::bordered().title(" System prompt ").inner(body);
                let row = draft[..*cursor].chars().filter(|c| *c == '\n').count() as u16;
                let line = draft[..*cursor].rsplit('\n').next().unwrap_or_default();
                let column = line.chars().count() as u16;
                let offset = row.saturating_sub(inner.height.saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(draft.as_str())
                        .block(Block::bordered().title(" System prompt "))
                        .scroll((offset, 0)),
                    body,
                );
                if !busy && !model_focus {
                    frame.set_cursor_position((
                        inner.x + column.min(inner.width.saturating_sub(1)),
                        inner.y + row.saturating_sub(offset),
                    ));
                }
                frame.render_widget(
                    Paragraph::new(format!(
                        "Model (optional): {}{}\nEdited on {edited_at}\n{status}",
                        if *model_focus { "› " } else { "" },
                        if model.is_empty() {
                            backend_default.as_ref().map_or_else(
                                || "No model configured".into(),
                                |value| format!("{value} (default)"),
                            )
                        } else {
                            model.clone()
                        }
                    ))
                    .wrap(Wrap { trim: false })
                    .fg(Color::DarkGray),
                    notice,
                );
                frame.render_widget(
                    Paragraph::new("Tab switch field · Ctrl+S save · Ctrl+U clear · Esc back")
                        .dark_gray(),
                    footer,
                );
            }
            Self::Models {
                catalog,
                selected,
                notice: status,
                ..
            } => {
                frame.render_widget(
                    Paragraph::new(" SOLMU / MODEL\n\nSelect the model for this conversation")
                        .green(),
                    header,
                );
                let mut names = vec![format!(
                    "Default{}",
                    catalog
                        .as_ref()
                        .and_then(|value| value.default_model.as_ref())
                        .map_or(String::new(), |value| format!(" · {value}"))
                )];
                if let Some(catalog) = catalog {
                    names.extend(
                        catalog
                            .models
                            .iter()
                            .map(|model| format!("{}  ({})", model.name, model.id)),
                    );
                }
                let lines: Vec<ratatui::text::Line<'_>> = names
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        if index == *selected {
                            format!("› {name}").green().bold().into()
                        } else {
                            format!("  {name}").into()
                        }
                    })
                    .collect();
                frame.render_widget(Paragraph::new(lines), body);
                frame.render_widget(Paragraph::new(status.as_str()).dark_gray(), notice);
                frame.render_widget(
                    Paragraph::new(
                        "↑/↓ choose · Enter select · Esc back · /model <id> for a custom model",
                    )
                    .dark_gray(),
                    footer,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(text: &str) -> Profile {
        Profile {
            system_prompt: text.into(),
            model: None,
            backend_default_model: None,
            edited_at: "2026-09-27T00:00:00Z".into(),
        }
    }
    #[test]
    fn live_events_during_profile_loading_are_coalesced_and_drafts_are_preserved() {
        let mut page = Page::profile();
        assert!(!page.refresh());
        page.update(Event::Loaded(Ok(profile("First value"))));
        assert!(page.refresh_pending());
        assert!(!page.refresh());
        page.update(Event::Loaded(Ok(profile("Second value"))));
        assert!(page.refresh_pending());
        page.update(Event::Loaded(Ok(profile("Newest value"))));
        assert!(!page.refresh_pending());
        let Page::Profile { draft, .. } = &mut page else {
            unreachable!()
        };
        *draft = "Unsent preferences".into();
        assert!(!page.refresh());
        let Page::Profile { draft, notice, .. } = &page else {
            unreachable!()
        };
        assert_eq!(draft, "Unsent preferences");
        assert!(notice.contains("Your draft is unchanged"));
    }
}
