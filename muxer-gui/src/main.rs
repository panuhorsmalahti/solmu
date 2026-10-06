use iced::{
    Alignment, Element, Length, Subscription, Task, Theme,
    keyboard::{
        self, Event as KeyboardEvent,
        key::{Key, Named},
    },
    widget::{button, column, container, mouse_area, row, scrollable, text},
};
use serde_json::{Value, json};
use solmu_client::{Action, Api};
use solmu_desktop::{Desktop, Event as DesktopEvent};
use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

mod appearance;

pub fn main() -> iced::Result {
    iced::application(MuxerGui::new, MuxerGui::update, MuxerGui::view)
        .title("Muxer GUI")
        .theme(theme)
        .subscription(subscription)
        .window_size((1180.0, 760.0))
        .run()
}

struct MuxerGui {
    session: String,
    workspace: String,
    snapshot: Value,
    selected_pane: Option<u64>,
    screen: String,
    terminal_focused: bool,
    notice: String,
    show_space_types: bool,
    context_space: Option<u64>,
    desktops: HashMap<u64, Desktop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpaceType {
    Solmu,
    Terminal,
}

impl SpaceType {
    fn launch(self) -> Value {
        match self {
            Self::Solmu => json!({"kind":"solmu"}),
            Self::Terminal => json!({"kind":"shell","argv":[]}),
        }
    }
}

impl fmt::Display for SpaceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Solmu => "Solmu",
            Self::Terminal => "Terminal",
        })
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    TerminalFocus,
    TerminalKey(KeyboardEvent),
    ToggleSpaceMenu,
    SpaceContextMenu(u64),
    DeleteSpace(u64),
    CreateSpace(SpaceType),
    NewTab,
    Split,
    SelectPane(u64),
    FocusSpace(u64),
    FocusTab(u64),
    CloseTab(u64),
    ClosePane(u64),
    Desktop(u64, DesktopEvent),
    SnapshotLoaded(Result<Value, String>),
    ScreenLoaded(u64, Result<String, String>),
    OperationFinished(String, Result<(), String>),
}

impl MuxerGui {
    fn new() -> (Self, Task<Message>) {
        let state = Self {
            session: "default".into(),
            workspace: std::env::current_dir()
                .unwrap_or_default()
                .display()
                .to_string(),
            snapshot: Value::Null,
            selected_pane: None,
            screen: String::new(),
            terminal_focused: false,
            notice: "Starting the embedded workspace engine…".into(),
            show_space_types: false,
            context_space: None,
            desktops: HashMap::new(),
        };
        let task = state.snapshot_task();
        (state, task)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => self.snapshot_task(),
            Message::ToggleSpaceMenu => {
                self.show_space_types = !self.show_space_types;
                Task::none()
            }
            Message::SpaceContextMenu(id) => {
                self.context_space = Some(id);
                Task::none()
            }
            Message::DeleteSpace(id) => {
                self.context_space = None;
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"close","params":{"target":"space","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Space deleted".into(),
                )
            }
            Message::TerminalFocus => {
                self.terminal_focused = true;
                Task::none()
            }
            Message::TerminalKey(KeyboardEvent::KeyPressed {
                key,
                text,
                modifiers,
                ..
            }) => {
                if !self.terminal_focused || self.active_space_type() != SpaceType::Terminal {
                    return Task::none();
                }
                let Some(pane) = self
                    .selected_pane
                    .or_else(|| self.snapshot["active_pane"].as_u64())
                else {
                    return Task::none();
                };
                let close_shortcut =
                    modifiers.command() && matches!(key.as_ref(), Key::Character("c" | "C"));
                if close_shortcut {
                    return self.update(Message::ClosePane(pane));
                }
                let chord = match key {
                    Key::Named(named) => terminal_named_key(named).map(str::to_owned),
                    Key::Character(character)
                        if modifiers.control() || modifiers.alt() || modifiers.logo() =>
                    {
                        let modifier = if modifiers.control() {
                            "ctrl"
                        } else if modifiers.alt() {
                            "alt"
                        } else {
                            "super"
                        };
                        Some(format!("{modifier}-{character}"))
                    }
                    _ => None,
                };
                let session = self.session.clone();
                if let Some(chord) = chord {
                    if let Err(error) = request(
                        &session,
                        json!({"method":"send_keys","params":{"pane":pane,"keys":[chord]}}),
                    ) {
                        self.notice = error;
                    }
                    Task::none()
                } else if let Some(text) = text
                    .filter(|text| !text.is_empty())
                    .map(|text| text.to_string())
                {
                    if let Err(error) = request(
                        &session,
                        json!({"method":"send_text","params":{"pane":pane,"text":text}}),
                    ) {
                        self.notice = error;
                    }
                    Task::none()
                } else {
                    Task::none()
                }
            }
            Message::TerminalKey(_) => Task::none(),
            Message::CreateSpace(space_type) => {
                self.terminal_focused = false;
                self.show_space_types = false;
                let (session, cwd, launch) = (
                    self.session.clone(),
                    self.workspace.clone(),
                    space_type.launch(),
                );
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"create_space","params":{"cwd":cwd,"launch":launch,"focus":true}}),
                        )
                        .map(|_| ())
                    },
                    "Space created".into(),
                )
            }
            Message::NewTab => {
                self.terminal_focused = false;
                let (session, space, launch) = (
                    self.session.clone(),
                    self.snapshot["active_space"].as_u64(),
                    self.active_space_type().launch(),
                );
                if let Some(space) = space {
                    perform(
                        move || {
                            request(&session, json!({"method":"create_tab","params":{"space":space,"launch":launch,"focus":true}})).map(|_| ())
                        },
                        "Tab created".into(),
                    )
                } else {
                    Task::none()
                }
            }
            Message::Split => {
                if let Some(pane) = self
                    .selected_pane
                    .or_else(|| self.snapshot["active_pane"].as_u64())
                {
                    let (session, launch) =
                        (self.session.clone(), self.active_space_type().launch());
                    perform(
                        move || {
                            request(&session, json!({"method":"split_pane","params":{"pane":pane,"axis":"right","launch":launch,"focus":true}})).map(|_| ())
                        },
                        "Pane split".into(),
                    )
                } else {
                    Task::none()
                }
            }
            Message::FocusSpace(id) => {
                self.terminal_focused = false;
                self.context_space = None;
                self.show_space_types = false;
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"focus","params":{"target":"space","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Space selected".into(),
                )
            }
            Message::FocusTab(id) => {
                self.terminal_focused = false;
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"focus","params":{"target":"tab","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Tab selected".into(),
                )
            }
            Message::CloseTab(id) => {
                self.terminal_focused = false;
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"close","params":{"target":"tab","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Tab closed".into(),
                )
            }
            Message::SelectPane(id) => {
                self.terminal_focused = false;
                self.selected_pane = Some(id);
                self.screen.clear();
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"focus","params":{"target":"pane","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Pane selected".into(),
                )
            }
            Message::ClosePane(id) => {
                self.terminal_focused = false;
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"close","params":{"target":"pane","id":id}}),
                        )
                        .map(|_| ())
                    },
                    "Pane closed".into(),
                )
            }
            Message::SnapshotLoaded(result) => {
                match result {
                    Ok(snapshot) => {
                        self.snapshot = snapshot;
                        let spaces = self.snapshot["spaces"].as_array().map_or(0, Vec::len);
                        let panes = self.snapshot["panes"].as_array().map_or(0, Vec::len);
                        self.notice = format!("Workspace ready · {spaces} spaces · {panes} panes");
                        let panes = self.snapshot["panes"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        let pane_ids: std::collections::HashSet<_> = panes
                            .iter()
                            .filter_map(|pane| pane["id"].as_u64())
                            .collect();
                        self.desktops.retain(|id, _| pane_ids.contains(id));
                        let space_ids: std::collections::HashSet<_> = self.snapshot["spaces"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|space| space["id"].as_u64())
                            .collect();
                        if self
                            .context_space
                            .is_some_and(|id| !space_ids.contains(&id))
                        {
                            self.context_space = None;
                        }
                        let selected = self
                            .selected_pane
                            .filter(|id| panes.iter().any(|p| p["id"].as_u64() == Some(*id)))
                            .or_else(|| self.snapshot["active_pane"].as_u64())
                            .or_else(|| panes.first().and_then(|p| p["id"].as_u64()));
                        if selected != self.selected_pane {
                            self.selected_pane = selected;
                        }
                        let mut tasks = vec![self.ensure_embedded_desktop()];
                        if let Some(id) = selected {
                            tasks.push(self.read_pane(id));
                        }
                        return Task::batch(tasks);
                    }
                    Err(error) => self.notice = error,
                }
                Task::none()
            }
            Message::ScreenLoaded(id, result) => {
                if self.selected_pane == Some(id) {
                    self.screen = result.unwrap_or_else(|e| e);
                }
                Task::none()
            }
            Message::OperationFinished(label, result) => {
                self.notice = result.map_or_else(|e| e, |_| label);
                self.snapshot_task()
            }
            Message::Desktop(pane, event) => {
                self.desktops
                    .get_mut(&pane)
                    .map_or_else(Task::none, |desktop| {
                        desktop
                            .update(event)
                            .map(move |event| Message::Desktop(pane, event))
                    })
            }
        }
    }

    fn active_space_type(&self) -> SpaceType {
        if self.snapshot["spaces"]
            .as_array()
            .and_then(|spaces| {
                spaces
                    .iter()
                    .find(|space| space["id"].as_u64() == self.snapshot["active_space"].as_u64())
            })
            .and_then(|space| space["kind"].as_str())
            == Some("solmu")
        {
            SpaceType::Solmu
        } else {
            SpaceType::Terminal
        }
    }

    fn ensure_embedded_desktop(&mut self) -> Task<Message> {
        if self.active_space_type() != SpaceType::Solmu {
            return Task::none();
        }
        let Some(pane_id) = self.snapshot["active_pane"].as_u64() else {
            return Task::none();
        };
        let panes = self.snapshot["panes"].as_array();
        let pane = panes.and_then(|panes| {
            panes
                .iter()
                .find(|pane| pane["id"].as_u64() == Some(pane_id))
        });
        let workspace = pane
            .and_then(|pane| pane["cwd"].as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&self.workspace));
        let thread = pane
            .and_then(|pane| pane["thread"].as_str())
            .map(str::to_owned);
        if let Some(desktop) = self.desktops.get_mut(&pane_id) {
            if desktop.session.current.is_none()
                && let Some(thread) = thread
            {
                return desktop
                    .update(DesktopEvent::Action(Action::Open(thread)))
                    .map(move |event| Message::Desktop(pane_id, event));
            }
            return Task::none();
        }
        let (desktop, task) =
            Desktop::new_with_thread(Api::from_env().with_workspace(workspace), thread);
        self.desktops.insert(pane_id, desktop);
        task.map(move |event| Message::Desktop(pane_id, event))
    }

    fn snapshot_task(&self) -> Task<Message> {
        let session = self.session.clone();
        let workspace = PathBuf::from(&self.workspace);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || snapshot(&session, workspace))
                    .await
                    .map_err(|e| e.to_string())?
            },
            Message::SnapshotLoaded,
        )
    }

    fn read_pane(&self, id: u64) -> Task<Message> {
        let session = self.session.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let output = request(
                        &session,
                        json!({"method":"read","params":{"pane":id,"ansi":true}}),
                    )?;
                    Ok(output["text"].as_str().unwrap_or_default().to_string())
                })
                .await
                .map_err(|e| e.to_string())?
            },
            move |result| Message::ScreenLoaded(id, result),
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let spaces = self.snapshot["spaces"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let tabs = self.snapshot["tabs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let panes = self.snapshot["panes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let active_space = self.snapshot["active_space"].as_u64();
        let active_tab = self.snapshot["active_tab"].as_u64();
        let brand = row![
            container(text("M").size(15).color(iced::Color::WHITE))
                .padding([5, 10])
                .style(|_| container::Style {
                    background: Some(appearance::PRIMARY.into()),
                    border: iced::Border {
                        radius: 9.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            column![
                text("muxer").size(20),
                text("WORKSPACE CONTROL").size(9).color(appearance::MUTED)
            ]
            .spacing(1)
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mut spaces_section = column![
            row![
                text("SPACES").size(10).color(appearance::MUTED),
                iced::widget::Space::new().width(Length::Fill),
                button(text("＋").size(16))
                    .on_press(Message::ToggleSpaceMenu)
                    .style(appearance::ghost)
                    .padding([3, 8])
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);
        if self.show_space_types {
            spaces_section = spaces_section.push(
                container(
                    column![
                        button(text("Terminal").size(12))
                            .on_press(Message::CreateSpace(SpaceType::Terminal))
                            .style(appearance::ghost)
                            .padding([7, 10])
                            .width(Length::Fill),
                        button(text("Solmu").size(12))
                            .on_press(Message::CreateSpace(SpaceType::Solmu))
                            .style(appearance::ghost)
                            .padding([7, 10])
                            .width(Length::Fill),
                    ]
                    .spacing(3),
                )
                .padding(4)
                .style(appearance::panel),
            );
        }
        for space in &spaces {
            let id = space["id"].as_u64().unwrap_or_default();
            let selected = Some(id) == active_space;
            let name = space["name"].as_str().unwrap_or("Workspace").to_string();
            let cwd = space["cwd"].as_str().unwrap_or("").to_string();
            let kind = if space["kind"].as_str() == Some("solmu") {
                "Solmu"
            } else {
                "Terminal"
            };
            spaces_section = spaces_section.push(
                mouse_area(
                    button(
                        column![
                            row![
                                text("▰").size(12).color(if selected {
                                    appearance::PRIMARY
                                } else {
                                    appearance::MUTED
                                }),
                                text(name).size(13),
                                iced::widget::Space::new().width(Length::Fill),
                                text(kind).size(9).color(appearance::MUTED)
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            text(cwd).size(10).color(appearance::MUTED),
                        ]
                        .spacing(3),
                    )
                    .on_press(Message::FocusSpace(id))
                    .style(move |theme, state| appearance::navigation(theme, state, selected))
                    .padding([9, 10])
                    .width(Length::Fill),
                )
                .on_right_press(Message::SpaceContextMenu(id)),
            );
            if self.context_space == Some(id) {
                spaces_section = spaces_section.push(
                    container(
                        button(
                            text("Delete space")
                                .size(11)
                                .color(iced::Color::from_rgb8(190, 65, 65)),
                        )
                        .on_press(Message::DeleteSpace(id))
                        .style(appearance::ghost)
                        .padding([7, 10])
                        .width(Length::Fill),
                    )
                    .padding(4)
                    .style(appearance::panel)
                    .width(Length::Fill),
                );
            }
            if Some(id) == active_space
                && let Some(tab_id) = active_tab
            {
                let tab_panes: Vec<_> = panes
                    .iter()
                    .filter(|pane| pane["tab"].as_u64() == Some(tab_id))
                    .collect();
                if tab_panes.len() > 1 {
                    spaces_section =
                        spaces_section.push(text("PANES").size(9).color(appearance::MUTED));
                    for pane in tab_panes {
                        let pane_id = pane["id"].as_u64().unwrap_or_default();
                        let name = pane["name"]
                            .as_str()
                            .or_else(|| pane["launch"].as_str())
                            .unwrap_or("Pane")
                            .to_string();
                        let pane_selected = self.selected_pane == Some(pane_id);
                        let pane_state = pane["state"].as_str().unwrap_or("idle").to_string();
                        let item = button(
                            row![
                                text("> ").size(13).color(appearance::MUTED),
                                text(name).size(12),
                                iced::widget::Space::new().width(Length::Fill),
                                text(pane_state).size(9).color(appearance::MUTED),
                            ]
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::SelectPane(pane_id))
                        .style(move |theme, state| {
                            appearance::navigation(theme, state, pane_selected)
                        })
                        .padding([7, 10])
                        .width(Length::Fill);
                        spaces_section = spaces_section.push(item);
                    }
                }
            }
        }
        let sidebar = column![
            brand,
            button(
                row![text("＋").size(15), text("New space").size(12)]
                    .spacing(8)
                    .align_y(Alignment::Center)
            )
            .on_press(Message::ToggleSpaceMenu)
            .style(appearance::primary_button)
            .padding([10, 12])
            .width(Length::Fill),
            spaces_section,
            iced::widget::Space::new().height(Length::Fill),
        ]
        .spacing(16)
        .padding(18)
        .width(Length::Fixed(276.0))
        .height(Length::Fill);
        let active_space_name = spaces
            .iter()
            .find(|space| space["id"].as_u64() == active_space)
            .and_then(|space| space["name"].as_str())
            .unwrap_or("Workspace")
            .to_string();
        let active_space_type = self.active_space_type();
        let active_solmu = active_space_type == SpaceType::Solmu;
        let mut tab_panel = row![].spacing(5).align_y(Alignment::Center);
        for tab in tabs
            .iter()
            .filter(|tab| tab["space"].as_u64() == active_space)
        {
            let tab_id = tab["id"].as_u64().unwrap_or_default();
            let tab_selected = Some(tab_id) == active_tab;
            let tab_name = tab["name"].as_str().unwrap_or("Tab").to_string();
            tab_panel = tab_panel.push(
                container(
                    row![
                        button(text(tab_name).size(12))
                            .on_press(Message::FocusTab(tab_id))
                            .style(move |theme, state| {
                                appearance::navigation(theme, state, tab_selected)
                            })
                            .padding([9, 12]),
                        button(text("x").size(11))
                            .on_press(Message::CloseTab(tab_id))
                            .style(appearance::ghost)
                            .padding([7, 9]),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(1),
                )
                .style(move |_| container::Style {
                    background: Some(if tab_selected {
                        appearance::SURFACE.into()
                    } else {
                        appearance::SIDEBAR.into()
                    }),
                    border: iced::Border {
                        radius: iced::border::Radius {
                            top_left: 9.0,
                            top_right: 9.0,
                            bottom_left: 3.0,
                            bottom_right: 3.0,
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }
        tab_panel = tab_panel
            .push(
                button(text("+").size(16))
                    .on_press(Message::NewTab)
                    .style(appearance::ghost)
                    .padding([7, 10]),
            )
            .push(iced::widget::Space::new().width(Length::Fill));
        let tab_panel = container(tab_panel)
            .padding([5, 8])
            .width(Length::Fill)
            .style(appearance::sidebar);
        let toolbar = row![
            column![
                row![
                    text("Muxer").size(11).color(appearance::MUTED),
                    text("/ ").size(11).color(appearance::MUTED),
                    text(active_space_name.clone()).size(12),
                    text("/ ").size(11).color(appearance::MUTED),
                    text(active_space_type.to_string()).size(12)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                text("Your workspace, organized into focused terminal panes.")
                    .size(11)
                    .color(appearance::MUTED),
            ]
            .spacing(5),
            iced::widget::Space::new().width(Length::Fill),
            button(text("Split pane").size(11))
                .on_press(Message::Split)
                .style(appearance::ghost)
                .padding([8, 10]),
            button(text("Close").size(11))
                .on_press_maybe(self.selected_pane.map(Message::ClosePane))
                .style(appearance::ghost)
                .padding([8, 10]),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let panes_title = panes
            .iter()
            .find(|pane| pane["id"].as_u64() == self.selected_pane)
            .and_then(|pane| pane["name"].as_str().or_else(|| pane["launch"].as_str()))
            .unwrap_or("Terminal")
            .to_string();
        let pane_header = row![
            column![
                text(panes_title).size(14),
                text(if self.snapshot.is_null() {
                    "Starting the embedded workspace"
                } else {
                    &self.notice
                })
                .size(10)
                .color(appearance::MUTED)
            ]
            .spacing(4),
            iced::widget::Space::new().width(Length::Fill),
            text("●").size(10).color(if self.snapshot.is_null() {
                appearance::MUTED
            } else {
                appearance::PRIMARY
            }),
            text(if self.snapshot.is_null() {
                "OFFLINE"
            } else {
                "LIVE"
            })
            .size(9)
            .color(appearance::MUTED),
        ]
        .align_y(Alignment::Center)
        .spacing(7);
        let terminal_text = if self.screen.is_empty() {
            "Choose a pane to see its live terminal output.\n\nCreate a space or start a session to begin."
        } else {
            &self.screen
        };
        let screen_dimensions = panes
            .iter()
            .find(|pane| pane["id"].as_u64() == self.selected_pane)
            .map(|pane| {
                (
                    pane["rows"].as_u64().unwrap_or(24) as u16,
                    pane["cols"].as_u64().unwrap_or(80) as u16,
                )
            })
            .unwrap_or((24, 80));
        let screen = mouse_area(
            container(
                scrollable(terminal_view(
                    terminal_text,
                    screen_dimensions.0,
                    screen_dimensions.1,
                ))
                .height(Length::Fill),
            )
            .padding(18)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(appearance::terminal),
        )
        .on_press(Message::TerminalFocus);
        let terminal_panel = container(
            column![pane_header, screen]
                .spacing(10)
                .padding(16)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(appearance::panel);
        let terminal_content = column![toolbar, terminal_panel]
            .spacing(16)
            .padding([22, 24])
            .width(Length::Fill)
            .height(Length::Fill);
        let content: Element<'_, Message> = if active_solmu {
            if let Some(pane) = self.snapshot["active_pane"].as_u64()
                && let Some(desktop) = self.desktops.get(&pane)
            {
                let solmu_content = column![
                    row![
                        text(format!("{} · Solmu", active_space_name)).size(12),
                        iced::widget::Space::new().width(Length::Fill),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(8),
                    desktop
                        .view()
                        .map(move |event| Message::Desktop(pane, event)),
                ]
                .spacing(10)
                .padding([12, 18])
                .width(Length::Fill)
                .height(Length::Fill);
                solmu_content.into()
            } else {
                container(text("Opening Solmu…").size(18).color(appearance::MUTED))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            }
        } else {
            terminal_content.into()
        };
        let content = column![tab_panel, content]
            .spacing(0)
            .padding([16, 24])
            .width(Length::Fill)
            .height(Length::Fill);
        row![
            container(scrollable(sidebar).height(Length::Fill))
                .height(Length::Fill)
                .style(appearance::sidebar),
            content
        ]
        .height(Length::Fill)
        .into()
    }
}

fn theme(_: &MuxerGui) -> Theme {
    appearance::theme()
}

fn subscription(state: &MuxerGui) -> Subscription<Message> {
    let mut subscriptions =
        vec![iced::time::every(std::time::Duration::from_millis(1200)).map(|_| Message::Tick)];
    if state.active_space_type() == SpaceType::Solmu
        && let Some(pane) = state.snapshot["active_pane"].as_u64()
        && let Some(desktop) = state.desktops.get(&pane)
    {
        subscriptions.push(
            desktop
                .subscription()
                .with(pane)
                .map(|(pane, event)| Message::Desktop(pane, event)),
        );
    }
    if state.active_space_type() == SpaceType::Terminal && state.terminal_focused {
        subscriptions.push(keyboard::listen().map(Message::TerminalKey));
    }
    Subscription::batch(subscriptions)
}

fn terminal_named_key(key: Named) -> Option<&'static str> {
    Some(match key {
        Named::Enter => "enter",
        Named::Tab => "tab",
        Named::Escape => "esc",
        Named::Backspace => "backspace",
        Named::Delete => "delete",
        Named::Insert => "insert",
        Named::ArrowUp => "up",
        Named::ArrowDown => "down",
        Named::ArrowLeft => "left",
        Named::ArrowRight => "right",
        Named::Home => "home",
        Named::End => "end",
        Named::PageUp => "pageup",
        Named::PageDown => "pagedown",
        _ => return None,
    })
}

fn terminal_view(screen: &str, rows: u16, cols: u16) -> Element<'static, Message> {
    use iced::widget::text::{Rich, Span};

    let mut parser = vt100::Parser::new(rows.max(1), cols.max(1), 0);
    parser.process(screen.as_bytes());
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let cursor = screen.cursor_position();
    let mut output = column![].spacing(0);
    for row in 0..rows {
        let mut spans: Vec<Span<'static, (), iced::Font>> = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<(iced::Color, iced::Color, bool, bool)> = None;
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let is_cursor = cursor == (row, col);
            let (fg, bg) = if cell.inverse() || is_cursor {
                (
                    terminal_color(cell.bgcolor(), true),
                    terminal_color(cell.fgcolor(), false),
                )
            } else {
                (
                    terminal_color(cell.fgcolor(), true),
                    terminal_color(cell.bgcolor(), false),
                )
            };
            let style = (fg, bg, cell.bold(), cell.underline());
            if run_style.is_some_and(|current| current != style) {
                let (fg, bg, bold, underline) = run_style.unwrap();
                spans.push(
                    Span::new(std::mem::take(&mut run))
                        .font(iced::Font {
                            weight: if bold {
                                iced::font::Weight::Bold
                            } else {
                                iced::font::Weight::Normal
                            },
                            ..iced::Font::MONOSPACE
                        })
                        .size(13)
                        .color(fg)
                        .background(bg)
                        .underline(underline),
                );
            }
            run_style = Some(style);
            let contents = cell.contents();
            run.push_str(if contents.is_empty() { " " } else { contents });
        }
        if let Some((fg, bg, bold, underline)) = run_style {
            spans.push(
                Span::new(run)
                    .font(iced::Font {
                        weight: if bold {
                            iced::font::Weight::Bold
                        } else {
                            iced::font::Weight::Normal
                        },
                        ..iced::Font::MONOSPACE
                    })
                    .size(13)
                    .color(fg)
                    .background(bg)
                    .underline(underline),
            );
        }
        output = output.push(
            Rich::<(), Message>::with_spans(spans)
                .font(iced::Font::MONOSPACE)
                .size(13)
                .width(Length::Fill),
        );
    }
    output.width(Length::Fill).into()
}

fn terminal_color(color: vt100::Color, foreground: bool) -> iced::Color {
    match color {
        vt100::Color::Default => {
            if foreground {
                iced::color!(0xd5e0dc)
            } else {
                iced::color!(0x111a20)
            }
        }
        vt100::Color::Rgb(r, g, b) => iced::Color::from_rgb8(r, g, b),
        vt100::Color::Idx(index) => {
            const ANSI: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (205, 49, 49),
                (13, 188, 121),
                (229, 229, 16),
                (36, 114, 200),
                (188, 63, 188),
                (17, 168, 205),
                (229, 229, 229),
                (102, 102, 102),
                (241, 76, 76),
                (35, 209, 139),
                (245, 245, 67),
                (59, 142, 234),
                (214, 112, 214),
                (41, 184, 219),
                (255, 255, 255),
            ];
            let (r, g, b) = match index {
                0..=15 => ANSI[usize::from(index)],
                16..=231 => {
                    let cube = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
                    let value = index - 16;
                    (cube(value / 36), cube(value / 6 % 6), cube(value % 6))
                }
                _ => {
                    let grey = 8 + (index - 232) * 10;
                    (grey, grey, grey)
                }
            };
            iced::Color::from_rgb8(r, g, b)
        }
    }
}

fn perform<F>(work: F, label: String) -> Task<Message>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(work)
                .await
                .map_err(|e| e.to_string())?
        },
        move |result| Message::OperationFinished(label, result),
    )
}

static SESSION: OnceLock<Mutex<Option<solmu_muxer::gui::GuiSession>>> = OnceLock::new();

fn with_session<T>(
    name: &str,
    cwd: PathBuf,
    work: impl FnOnce(&mut solmu_muxer::gui::GuiSession) -> Result<T, String>,
) -> Result<T, String> {
    let slot = SESSION.get_or_init(|| Mutex::new(None));
    let mut slot = slot
        .lock()
        .map_err(|_| "Workspace engine lock was poisoned".to_string())?;
    if slot.as_ref().is_none_or(|session| session.name() != name) {
        *slot = None;
        let executable = solmu_binary()?;
        *slot = Some(
            solmu_muxer::gui::GuiSession::open(name, executable, cwd)
                .map_err(|error| error.to_string())?,
        );
    }
    work(slot.as_mut().expect("session was initialized"))
}

fn snapshot(name: &str, cwd: PathBuf) -> Result<Value, String> {
    with_session(name, cwd, |session| session.refresh())
}

fn request(name: &str, request: Value) -> Result<Value, String> {
    with_session(
        name,
        std::env::current_dir().unwrap_or_default(),
        |session| session.request(request),
    )
}

fn solmu_binary() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("SOLMU_CLI_PATH") {
        return Ok(PathBuf::from(path));
    }
    let sibling = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name(format!("solmu{}", std::env::consts::EXE_SUFFIX));
    if sibling.is_file() {
        return Ok(sibling);
    }
    Ok(PathBuf::from(format!(
        "solmu{}",
        std::env::consts::EXE_SUFFIX
    )))
}
