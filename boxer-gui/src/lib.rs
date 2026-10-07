use iced::futures::SinkExt;
use iced::{
    Element, Length, Subscription, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_input},
};
use notify::{EventKind, RecursiveMode, Watcher};
use serde_json::Value;
#[cfg(windows)]
use std::process::Stdio;
use std::{path::PathBuf, process::Command, time::Duration};

#[derive(Clone, Debug)]
pub struct Launch {
    pub id: String,
    pub status: String,
    pub attachment: String,
    pub can_stop: bool,
    pub pid: u32,
    pub command: String,
    pub workspace: String,
    pub created: u128,
}

#[derive(Clone, Debug)]
pub enum Message {
    SessionsChanged,
    Loaded(Result<Vec<Launch>, String>),
    CommandChanged(String),
    ProfileChanged(String),
    WorkspaceChanged(String),
    Launch,
    Launched(Result<(), String>),
    Stop(String),
    Stopped(Result<(), String>),
}

pub struct BoxerGui {
    root: PathBuf,
    launches: Vec<Launch>,
    command: String,
    profile: String,
    workspace: String,
    loaded: bool,
    busy: bool,
    notice: String,
}

pub fn application()
-> iced::Application<impl iced::Program<State = BoxerGui, Message = Message, Theme = Theme>> {
    application_with_root(session_root())
}

pub fn application_with_root(
    root: PathBuf,
) -> iced::Application<impl iced::Program<State = BoxerGui, Message = Message, Theme = Theme>> {
    iced::application(
        move || {
            let current = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
                .to_string();
            (
                BoxerGui {
                    root: root.clone(),
                    launches: Vec::new(),
                    command: "solmu".into(),
                    profile: "solmu".into(),
                    workspace: current,
                    loaded: false,
                    busy: false,
                    notice: String::new(),
                },
                Task::perform(load(root.clone()), Message::Loaded),
            )
        },
        BoxerGui::update,
        BoxerGui::view,
    )
    .title("Boxer")
    .theme(solmu_desktop::theme())
    .subscription(BoxerGui::subscription)
    .window_size((980.0, 720.0))
}

impl BoxerGui {
    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(self.root.clone(), |root| {
            let root = root.clone();
            iced::stream::channel(
                8,
                move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
                    let mut watcher = match notify::recommended_watcher(
                        move |event: notify::Result<notify::Event>| {
                            if event.is_ok_and(|event| {
                                matches!(
                                    event.kind,
                                    EventKind::Create(_)
                                        | EventKind::Modify(_)
                                        | EventKind::Remove(_)
                                )
                            }) {
                                let _ = sender.blocking_send(());
                            }
                        },
                    ) {
                        Ok(watcher) => watcher,
                        Err(error) => {
                            let _ = output
                                .send(Message::Loaded(Err(format!(
                                    "Cannot watch Boxer sessions: {error}"
                                ))))
                                .await;
                            return;
                        }
                    };
                    if let Err(error) = std::fs::create_dir_all(&root) {
                        let _ = output
                            .send(Message::Loaded(Err(format!(
                                "Cannot create Boxer session directory: {error}"
                            ))))
                            .await;
                        return;
                    }
                    if let Err(error) = watcher.watch(&root, RecursiveMode::NonRecursive) {
                        let _ = output
                            .send(Message::Loaded(Err(format!(
                                "Cannot watch Boxer sessions: {error}"
                            ))))
                            .await;
                        return;
                    }
                    // A single startup read handles changes made while the GUI was closed.
                    let _ = output.send(Message::SessionsChanged).await;
                    loop {
                        if receiver.recv().await.is_none() {
                            break;
                        }
                        // Coalesce the atomic rename/write notifications into one read.
                        tokio::time::sleep(Duration::from_millis(35)).await;
                        while receiver.try_recv().is_ok() {}
                        if output.send(Message::SessionsChanged).await.is_err() {
                            break;
                        }
                    }
                },
            )
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SessionsChanged => Task::perform(load(self.root.clone()), Message::Loaded),
            Message::Loaded(result) => {
                self.loaded = true;
                match result {
                    Ok(launches) => self.launches = launches,
                    Err(error) => self.notice = error,
                }
                Task::none()
            }
            Message::CommandChanged(value) => {
                self.command = value;
                Task::none()
            }
            Message::ProfileChanged(value) => {
                self.profile = value;
                Task::none()
            }
            Message::WorkspaceChanged(value) => {
                self.workspace = value;
                Task::none()
            }
            Message::Launch => {
                let root = self.root.clone();
                let command = self.command.trim().to_owned();
                let profile = self.profile.trim().to_owned();
                let workspace = self.workspace.trim().to_owned();
                if command.is_empty() || workspace.is_empty() || self.busy {
                    return Task::none();
                }
                self.busy = true;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            launch(&root, &workspace, &profile, &command)
                        })
                        .await
                        .unwrap_or_else(|error| Err(error.to_string()))
                    },
                    Message::Launched,
                )
            }
            Message::Launched(result) => {
                self.busy = false;
                self.notice = result.map_or_else(|error| error, |_| "Launch started".into());
                Task::perform(load(self.root.clone()), Message::Loaded)
            }
            Message::Stop(id) => {
                self.busy = true;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || stop(&id))
                            .await
                            .unwrap_or_else(|error| Err(error.to_string()))
                    },
                    Message::Stopped,
                )
            }
            Message::Stopped(result) => {
                self.busy = false;
                self.notice = result.map_or_else(|error| error, |_| "Launch stopped".into());
                Task::perform(load(self.root.clone()), Message::Loaded)
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let title = row![
            text("Boxer").size(30),
            iced::widget::space().width(Length::Fill),
            text("LOCAL PROCESS SANDBOX").size(12),
        ]
        .align_y(iced::Alignment::Center);
        let launch_form = container(
            column![
                text("Start a sandboxed process").size(20),
                row![
                    text_input("Profile (optional)", &self.profile)
                        .on_input(Message::ProfileChanged),
                    text_input("Program", &self.command).on_input(Message::CommandChanged),
                    text_input("Workspace folder", &self.workspace)
                        .on_input(Message::WorkspaceChanged),
                    button(if self.busy { "Working…" } else { "Launch" })
                        .on_press_maybe((!self.busy).then_some(Message::Launch))
                        .style(solmu_desktop::appearance::primary),
                ]
                .spacing(10),
                text("Choose an installed Boxer profile, program, and workspace.").size(13),
            ]
            .spacing(14),
        )
        .padding(20)
        .style(card);

        let mut list = column![].spacing(10);
        if !self.loaded {
            list = list.push(
                container(text("Reading local Boxer sessions…").size(16))
                    .padding(22)
                    .width(Length::Fill)
                    .style(card),
            );
        } else if self.launches.is_empty() {
            list = list.push(
                container(
                    column![
                        text("No Boxer launches yet").size(19),
                        text("Launch an agent or command above. Active launches from the Boxer CLI appear here immediately too.").size(14),
                    ]
                    .spacing(8),
                )
                .padding(22)
                .width(Length::Fill)
                .style(card),
            );
        }
        for launch in &self.launches {
            let state_color = if launch.status == "running" {
                "●"
            } else {
                "○"
            };
            let details = column![
                row![
                    text(format!("{state_color}  {}", launch.command)).size(17),
                    iced::widget::space().width(Length::Fill),
                    text(format!("{} · PID {}", launch.status, launch.pid)).size(13),
                ]
                .align_y(iced::Alignment::Center),
                text(format!(
                    "{}   ·   {}   ·   {}",
                    launch.workspace, launch.attachment, launch.id
                ))
                .size(12),
            ]
            .spacing(8);
            let actions = if launch.status == "running" && launch.can_stop {
                button("Stop")
                    .on_press_maybe((!self.busy).then(|| Message::Stop(launch.id.clone())))
                    .style(solmu_desktop::appearance::danger)
            } else {
                button(if launch.status == "running" {
                    "Use terminal"
                } else {
                    "Finished"
                })
                .style(solmu_desktop::appearance::ghost)
            };
            list = list.push(
                container(
                    row![details, actions]
                        .spacing(18)
                        .align_y(iced::Alignment::Center),
                )
                .padding(18)
                .width(Length::Fill)
                .style(card),
            );
        }

        container(
            column![
                title,
                text("Manage attached and detached Boxer processes from one live view.").size(15),
                launch_form,
                row![
                    text("Launches").size(21),
                    iced::widget::space().width(Length::Fill),
                    text(format!("{} tracked", self.launches.len())).size(13)
                ],
                scrollable(list).height(Length::Fill),
                text(&self.notice).size(13),
            ]
            .spacing(18),
        )
        .padding(28)
        .into()
    }
}

fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Color::WHITE.into()),
        border: iced::Border {
            color: iced::color!(0xdfe6dd),
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

fn session_root() -> PathBuf {
    std::env::var_os("BOXER_SESSIONS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".boxer").join("sessions"))
        })
        .unwrap_or_else(|| PathBuf::from(".boxer/sessions"))
}

async fn load(root: PathBuf) -> Result<Vec<Launch>, String> {
    tokio::task::spawn_blocking(move || read_launches(&root))
        .await
        .map_err(|error| error.to_string())?
}

fn read_launches(root: &std::path::Path) -> Result<Vec<Launch>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(root).map_err(|error| error.to_string())?;
    let mut launches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(
            &std::fs::read(entry.path()).map_err(|error| error.to_string())?,
        ) else {
            continue;
        };
        if value.get("id").and_then(Value::as_str).is_none() {
            continue;
        }
        let status = value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let detached = value["detached"].as_bool().unwrap_or(true);
        launches.push(Launch {
            id: value["id"].as_str().unwrap_or_default().to_owned(),
            status,
            attachment: if value["attached"].as_bool().unwrap_or(false) {
                "attached terminal".into()
            } else if detached {
                "detached".into()
            } else {
                "attached launch".into()
            },
            can_stop: detached || cfg!(windows),
            pid: value["pid"].as_u64().unwrap_or_default() as u32,
            command: value["command"].as_str().unwrap_or("unknown").to_owned(),
            workspace: value["workspace"].as_str().unwrap_or("").to_owned(),
            created: value["created_unix_ms"].as_u64().unwrap_or_default() as u128,
        });
    }
    launches.sort_by_key(|launch| std::cmp::Reverse(launch.created));
    Ok(launches)
}

fn launch(
    root: &std::path::Path,
    workspace: &str,
    profile: &str,
    command: &str,
) -> Result<(), String> {
    let boxer = find_boxer()?;
    let mut process = Command::new(boxer);
    #[cfg(unix)]
    process.arg("--detached");
    if !profile.is_empty() {
        process.args(["--profile", profile]);
    }
    process
        .arg("--cwd")
        .arg(workspace)
        .arg("--")
        .arg(command)
        .env("BOXER_SESSIONS_DIR", root);
    #[cfg(unix)]
    {
        let output = process
            .output()
            .map_err(|error| format!("Could not start Boxer: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    }
    #[cfg(windows)]
    {
        process
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not start Boxer: {error}"))
    }
}

fn stop(id: &str) -> Result<(), String> {
    let boxer = find_boxer()?;
    let output = Command::new(boxer)
        .args(["stop", id, "--force"])
        .output()
        .map_err(|error| format!("Could not stop launch: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn find_boxer() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("BOXER_PATH") {
        return Ok(path.into());
    }
    let name = if cfg!(windows) { "boxer.exe" } else { "boxer" };
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(name);
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| "Could not find boxer on PATH. Set BOXER_PATH to its executable.".into())
}
