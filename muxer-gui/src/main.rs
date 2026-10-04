use iced::{
    Alignment, Element, Length, Subscription, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_input},
};
use serde_json::{Value, json};
use std::{
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
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    SessionChanged(String),
    WorkspaceChanged(String),
    InputChanged(String),
    Start,
    NewSpace,
    NewTab,
    Split,
    SelectPane(u64),
    FocusSpace(u64),
    FocusTab(u64),
    Send,
    ClosePane(u64),
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
                let (session, cwd) = (self.session.clone(), self.workspace.clone());
                perform(
                    move || {
                        request(
                            &session,
                            json!({"method":"create_space","params":{"cwd":cwd,"focus":true}}),
                        )
                        .map(|_| ())
                    },
                    "Space created".into(),
                )
            }
            Message::NewTab => {
                let (session, space) =
                    (self.session.clone(), self.snapshot["active_space"].as_u64());
                if let Some(space) = space {
                    perform(
                        move || {
                            request(&session, json!({"method":"create_tab","params":{"space":space,"focus":true}})).map(|_| ())
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
                    let session = self.session.clone();
                    perform(
                        move || {
                            request(&session, json!({"method":"split_pane","params":{"pane":pane,"axis":"right","focus":true}})).map(|_| ())
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
                        return selected.map_or(Task::none(), |id| self.read_pane(id));
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
        }
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
                    Ok(output["result"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string())
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
        let mut sidebar = column![
            text("MUXER").size(17),
            text("Spaces").size(13),
            button("＋  New space").on_press(Message::NewSpace)
        ]
        .spacing(10)
        .padding(14)
        .width(Length::Fixed(245.0));
        for space in &spaces {
            let id = space["id"].as_u64().unwrap_or_default();
            let label = format!(
                "{}  ·  {}",
                space["name"].as_str().unwrap_or("Workspace"),
                space["cwd"].as_str().unwrap_or("")
            );
            sidebar = sidebar.push(
                button(text(label).size(13))
                    .on_press(Message::FocusSpace(id))
                    .width(Length::Fill),
            );
            if Some(id) == active_space {
                for tab in tabs.iter().filter(|tab| tab["space"].as_u64() == Some(id)) {
                    let tab_id = tab["id"].as_u64().unwrap_or_default();
                    sidebar = sidebar.push(
                        button(text(format!(
                            "    {}",
                            tab["name"].as_str().unwrap_or("Tab")
                        )))
                        .on_press(Message::FocusTab(tab_id))
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
                                .unwrap_or("Pane");
                            let label = format!(
                                "{}  ·  {}",
                                name,
                                pane["state"].as_str().unwrap_or("idle")
                            );
                            let item = button(text(label))
                                .on_press(Message::SelectPane(pane_id))
                                .width(Length::Fill);
                            sidebar = sidebar.push(item);
                        }
                    }
                }
            }
        }
        sidebar = sidebar.push(text("Session").size(13));
        sidebar =
            sidebar.push(text_input("default", &self.session).on_input(Message::SessionChanged));
        sidebar = sidebar.push(
            text_input("Workspace path", &self.workspace).on_input(Message::WorkspaceChanged),
        );
        if self.snapshot.is_null() {
            sidebar = sidebar.push(button("Start Muxer session").on_press(Message::Start));
        }
        let toolbar = row![
            button("＋ Space").on_press(Message::NewSpace),
            button("＋ Tab").on_press(Message::NewTab),
            button("Split right").on_press(Message::Split),
            button("Close pane").on_press_maybe(self.selected_pane.map(Message::ClosePane)),
            text(&self.notice).size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let screen = container(
            scrollable(
                text(if self.screen.is_empty() {
                    "Select a pane to view its live terminal screen."
                } else {
                    &self.screen
                })
                .font(iced::Font::MONOSPACE)
                .size(14),
            )
            .height(Length::Fill),
        )
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(
                0.07, 0.08, 0.1,
            ))),
            ..Default::default()
        });
        let input = row![
            text_input("Type into selected pane…", &self.input)
                .on_input(Message::InputChanged)
                .on_submit(Message::Send)
                .width(Length::Fill),
            button("Send ↵").on_press(Message::Send)
        ]
        .spacing(8);
        let content = column![toolbar, screen, input]
            .spacing(12)
            .padding(18)
            .width(Length::Fill)
            .height(Length::Fill);
        row![
            container(scrollable(sidebar).height(Length::Fill)).height(Length::Fill),
            content
        ]
        .height(Length::Fill)
        .into()
    }
}

fn theme(_: &MuxerGui) -> Theme {
    appearance::theme()
}

fn subscription(_: &MuxerGui) -> Subscription<Message> {
    iced::time::every(std::time::Duration::from_millis(1200)).map(|_| Message::Tick)
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
    let output = muxer_command(&["api", "snapshot", "--session", session, "--json"])?;
    Ok(output["result"].clone())
}

fn request(session: &str, request: Value) -> Result<Value, String> {
    let request = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    let output = muxer_command(&["api", "request", &request, "--session", session, "--json"])?;
    Ok(output["result"].clone())
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
