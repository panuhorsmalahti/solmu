use iced::{
    Alignment, Element, Length, Subscription, Task, Theme,
    widget::{button, column, container, pick_list, row, scrollable, text, text_input},
};
use serde_json::{Value, json};
use solmu_client::{Action, Api};
use solmu_desktop::{Desktop, Event as DesktopEvent};
use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    process::{Command, Stdio},
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
    input: String,
    notice: String,
    new_space_type: SpaceType,
    desktops: HashMap<u64, Desktop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpaceType {
    Solmu,
    Terminal,
}

impl SpaceType {
    const ALL: [Self; 2] = [Self::Solmu, Self::Terminal];

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
    SessionChanged(String),
    WorkspaceChanged(String),
    NewSpaceTypeChanged(SpaceType),
    InputChanged(String),
    Start,
    NewSpace,
    NewTab,
    Split,
    SelectPane(u64),
    FocusSpace(u64),
    FocusTab(u64),
    CloseTab(u64),
    Send,
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
            input: String::new(),
            notice: "Connect to a running Muxer session, or start one here.".into(),
            new_space_type: SpaceType::Solmu,
            desktops: HashMap::new(),
        };
        let task = state.snapshot_task();
        (state, task)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => self.snapshot_task(),
            Message::SessionChanged(value) => {
                self.session = value;
                self.snapshot = Value::Null;
                self.selected_pane = None;
                self.screen.clear();
                Task::none()
            }
            Message::WorkspaceChanged(value) => {
                self.workspace = value;
                Task::none()
            }
            Message::NewSpaceTypeChanged(space_type) => {
                self.new_space_type = space_type;
                Task::none()
            }
            Message::InputChanged(value) => {
                self.input = value;
                Task::none()
            }
            Message::Start => {
                let name = self.session.clone();
                let cwd = self.workspace.clone();
                perform(
                    move || {
                        muxer_raw(&["server", "start", "--session", &name, "--cwd", &cwd])
                            .map(|_| ())
                    },
                    "Muxer session started".into(),
                )
            }
            Message::NewSpace => {
                let (session, cwd, launch) = (
                    self.session.clone(),
                    self.workspace.clone(),
                    self.new_space_type.launch(),
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
            Message::Send => {
                let Some(pane) = self
                    .selected_pane
                    .or_else(|| self.snapshot["active_pane"].as_u64())
                else {
                    return Task::none();
                };
                let text = std::mem::take(&mut self.input);
                let session = self.session.clone();
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"send_text","params":{"pane":pane,"text":text}}),
                        )?;
                        request(
                            &session,
                            json!({"method":"send_keys","params":{"pane":pane,"keys":["enter"]}}),
                        )?;
                        Ok(())
                    },
                    "Sent to pane".into(),
                )
            }
            Message::ClosePane(id) => {
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
                        self.notice = format!("Connected · {spaces} spaces · {panes} panes");
                        let panes = self.snapshot["panes"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
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
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || snapshot(&session))
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
                    let output = muxer_command(&[
                        "pane",
                        "read",
                        &id.to_string(),
                        "--json",
                        "--session",
                        &session,
                    ])?;
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

        let status = row![
            text("●").size(11).color(if self.snapshot.is_null() {
                appearance::MUTED
            } else {
                appearance::PRIMARY
            }),
            column![
                text(if self.snapshot.is_null() {
                    "Not connected"
                } else {
                    "Session active"
                })
                .size(12),
                text(format!("{} · local", self.session))
                    .size(10)
                    .color(appearance::MUTED),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            text("⌄").size(14).color(appearance::MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut spaces_section = column![
            row![
                text("SPACES").size(10).color(appearance::MUTED),
                iced::widget::Space::new().width(Length::Fill),
                pick_list(
                    SpaceType::ALL,
                    Some(self.new_space_type),
                    Message::NewSpaceTypeChanged,
                )
                .width(Length::Fixed(104.0)),
                button(text("＋").size(16))
                    .on_press(Message::NewSpace)
                    .style(appearance::ghost)
                    .padding([3, 8])
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);
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
            );
            if Some(id) == active_space {
                for tab in tabs.iter().filter(|tab| tab["space"].as_u64() == Some(id)) {
                    let tab_id = tab["id"].as_u64().unwrap_or_default();
                    let tab_selected = Some(tab_id) == active_tab;
                    let tab_name = tab["name"].as_str().unwrap_or("Tab").to_string();
                    spaces_section = spaces_section.push(
                        button(
                            row![
                                text("▸").size(11).color(appearance::MUTED),
                                text(tab_name).size(12)
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::FocusTab(tab_id))
                        .style(move |theme, state| {
                            appearance::navigation(theme, state, tab_selected)
                        })
                        .padding([7, 10])
                        .width(Length::Fill),
                    );
                    if Some(tab_id) == active_tab {
                        for pane in panes
                            .iter()
                            .filter(|pane| pane["tab"].as_u64() == Some(tab_id))
                        {
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
                                    text("›").size(15).color(appearance::MUTED),
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
        }
        let session_fields = column![
            text("SESSION").size(10).color(appearance::MUTED),
            text_input("Session name", &self.session)
                .on_input(Message::SessionChanged)
                .style(appearance::input),
            text("WORKSPACE").size(10).color(appearance::MUTED),
            text_input("Project folder", &self.workspace)
                .on_input(Message::WorkspaceChanged)
                .style(appearance::input),
        ]
        .spacing(7);
        let mut sidebar = column![
            brand,
            container(status).padding(11).style(appearance::panel),
            button(
                row![text("＋").size(15), text("New space").size(12)]
                    .spacing(8)
                    .align_y(Alignment::Center)
            )
            .on_press(Message::NewSpace)
            .style(appearance::primary_button)
            .padding([10, 12])
            .width(Length::Fill),
            spaces_section,
            iced::widget::Space::new().height(Length::Fill),
            container(session_fields)
                .padding(11)
                .style(appearance::panel),
        ]
        .spacing(16)
        .padding(18)
        .width(Length::Fixed(276.0))
        .height(Length::Fill);
        if self.snapshot.is_null() {
            sidebar = sidebar.push(
                button(text("Start Muxer session").size(12))
                    .on_press(Message::Start)
                    .style(appearance::primary_button)
                    .padding([10, 12])
                    .width(Length::Fill),
            );
        }
        let active_space_name = spaces
            .iter()
            .find(|space| space["id"].as_u64() == active_space)
            .and_then(|space| space["name"].as_str())
            .unwrap_or("Workspace")
            .to_string();
        let active_space_type = self.active_space_type();
        let active_solmu = active_space_type == SpaceType::Solmu;
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
            button(text("＋  Tab").size(11))
                .on_press(Message::NewTab)
                .style(appearance::ghost)
                .padding([8, 10]),
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
                    "Waiting for a Muxer session"
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
        let screen = container(
            scrollable(
                text(terminal_text)
                    .font(iced::Font::MONOSPACE)
                    .size(13)
                    .color(iced::color!(0xd5e0dc)),
            )
            .height(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(appearance::terminal);
        let input = row![
            text("›").size(20).color(appearance::PRIMARY),
            text_input("Send a command to the selected pane…", &self.input)
                .on_input(Message::InputChanged)
                .on_submit(Message::Send)
                .style(appearance::input)
                .width(Length::Fill),
            button(text("Send  ↵").size(11))
                .on_press(Message::Send)
                .style(appearance::primary_button)
                .padding([9, 13])
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let terminal_panel = container(
            column![pane_header, screen, input]
                .spacing(14)
                .padding(16)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(appearance::panel);
        let content = column![
            toolbar,
            terminal_panel,
            text("MUXER CONTROL  ·  session state refreshes automatically")
                .size(9)
                .color(appearance::MUTED)
        ]
        .spacing(16)
        .padding([22, 24])
        .width(Length::Fill)
        .height(Length::Fill);
        let terminal_content = content;
        let content: Element<'_, Message> = if active_solmu {
            if let Some(pane) = self.snapshot["active_pane"].as_u64()
                && let Some(desktop) = self.desktops.get(&pane)
            {
                let solmu_content = column![
                    row![
                        text(format!("{} · Solmu", active_space_name)).size(12),
                        iced::widget::Space::new().width(Length::Fill),
                        button(text("New tab").size(11))
                            .on_press(Message::NewTab)
                            .style(appearance::ghost)
                            .padding([8, 10]),
                        button(text("Close tab").size(11))
                            .on_press_maybe(
                                self.snapshot["active_tab"].as_u64().map(Message::CloseTab)
                            )
                            .style(appearance::ghost)
                            .padding([8, 10]),
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
                .map(move |event| Message::Desktop(pane, event)),
        );
    }
    Subscription::batch(subscriptions)
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

fn snapshot(session: &str) -> Result<Value, String> {
    muxer_command(&["api", "snapshot", "--session", session, "--json"])
}

fn request(session: &str, request: Value) -> Result<Value, String> {
    let request = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    muxer_command(&["api", "request", &request, "--session", session, "--json"])
}

fn muxer_command(args: &[&str]) -> Result<Value, String> {
    let stdout = muxer_raw(args)?;
    parse_response(&stdout)
}

fn muxer_raw(args: &[&str]) -> Result<String, String> {
    let binary = muxer_binary()?;
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not run Muxer: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if let Ok(response) = serde_json::from_str::<Value>(stderr)
            && let Some(error) = response["error"].as_str()
        {
            return Err(error.to_string());
        }
        return Err(stderr.to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_response(stdout: &str) -> Result<Value, String> {
    let response: Value =
        serde_json::from_str(stdout).map_err(|e| format!("Invalid Muxer response: {e}"))?;
    if response["ok"].as_bool() != Some(true) {
        return Err(response["error"]
            .as_str()
            .unwrap_or("Muxer request failed")
            .to_string());
    }
    Ok(response["result"].clone())
}

fn muxer_binary() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("SOLMU_MUXER_PATH") {
        return Ok(PathBuf::from(path));
    }
    let sibling = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name(format!("muxer{}", std::env::consts::EXE_SUFFIX));
    if sibling.is_file() {
        return Ok(sibling);
    }
    Ok(PathBuf::from(format!(
        "muxer{}",
        std::env::consts::EXE_SUFFIX
    )))
}

#[cfg(test)]
mod tests {
    use super::parse_response;
    use serde_json::json;

    #[test]
    fn unwraps_muxer_control_response() {
        let result =
            parse_response(r#"{"ok":true,"result":{"active_pane":7,"panes":[{"id":7}]}}"#).unwrap();
        assert_eq!(result["active_pane"], 7);
        assert_eq!(result["panes"][0]["id"], 7);
    }

    #[test]
    fn reports_muxer_errors() {
        assert_eq!(
            parse_response(r#"{"ok":false,"error":"Session is not running"}"#),
            Err("Session is not running".into())
        );
    }

    #[test]
    fn rejects_invalid_control_output() {
        assert!(
            parse_response("not json")
                .unwrap_err()
                .starts_with("Invalid Muxer response:")
        );
        assert_eq!(parse_response(r#"{"ok":true}"#).unwrap(), json!(null));
    }
}
