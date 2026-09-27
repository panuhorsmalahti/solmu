use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use solmu_client::{Action, Api, Connection, Session, Update};
use std::io::Write as _;
use std::{error::Error, io};
mod automation;
use tokio::sync::mpsc;
mod settings;

const COMMANDS: &[&str] = &[
    "/new", "/threads", "/open", "/model", "/profile", "/rename", "/delete", "/help", "/stop",
    "/exit",
];
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn main() -> Result<(), Box<dyn Error>> {
    let mut thread = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Solmu CLI\n\nUsage: solmu [--thread ID]\n\nBy default, create a conversation in the current directory.\n--thread ID opens an existing conversation without creating another.\nSOLMU_BACKEND_URL defaults to http://127.0.0.1:3000.\nUse /help for conversation commands; Ctrl+L redraws the terminal."
                );
                return Ok(());
            }
            "--version" => {
                println!("Solmu CLI {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--thread" => {
                let id = args.next().ok_or("--thread requires a conversation ID")?;
                if id.len() != 36 || !id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
                    return Err("Invalid conversation ID".into());
                }
                thread = Some(id);
            }
            _ => return Err("Unknown option; use solmu --help".into()),
        }
    }
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env".into());
    }
    run(thread)
}

fn launch(
    session: &mut Session,
    action: Action,
    sender: &mpsc::UnboundedSender<Update>,
    runtime: &mut automation::Runtime,
) -> Option<tokio::task::JoinHandle<()>> {
    let sending = matches!(action, Action::Send(_));
    let updates = if matches!(action, Action::Refresh) {
        session.refresh()
    } else {
        session.begin(action)
    };
    if let Some(mut updates) = updates {
        if sending {
            runtime.start_if_needed(session);
        }
        let sender = sender.clone();
        Some(tokio::spawn(async move {
            while let Some(update) = updates.next().await {
                if sender.send(update).is_err() {
                    break;
                }
            }
        }))
    } else {
        None
    }
}

fn stop(
    session: &mut Session,
    active: &mut Option<tokio::task::JoinHandle<()>>,
    sender: &mpsc::UnboundedSender<Update>,
    runtime: &mut automation::Runtime,
) {
    runtime.cancel_queue();
    if !session.responding {
        return;
    }
    let response = active.take();
    session.stopping();
    let api = session.api.clone();
    let id = session
        .current
        .as_ref()
        .expect("selected thread")
        .id
        .clone();
    let sender = sender.clone();
    *active = Some(tokio::spawn(async move {
        match api.stop(&id).await {
            Ok(()) => {
                // Keep consuming tool results until the backend acknowledges
                // cancellation. The response stream sends Update::Stopped.
                if let Some(response) = response {
                    let _ = response.await;
                }
            }
            Err(error) => {
                if let Some(response) = response {
                    response.abort();
                }
                let _ = sender.send(Update::Failed(error));
            }
        }
    }));
}

#[tokio::main(worker_threads = 2)]
async fn run(thread: Option<String>) -> Result<(), Box<dyn Error>> {
    let mut terminal = ratatui::init();
    let result = async {
        let mut bridge = automation::Bridge::start()?;
        let mut runtime = automation::Runtime::default();
        let mut reported_metadata = String::new();
        let mut session = Session::new(Api::from_env().with_workspace(std::env::current_dir()?));
        let (settings_sender, mut settings_receiver) = mpsc::unbounded_channel();
        let mut page: Option<settings::Page> = None;
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut startup_thread = thread.clone();
        let initial = thread.map(Action::Open).unwrap_or_else(|| Action::New("New conversation".into()));
        let mut active = launch(&mut session, initial, &sender, &mut runtime);
        let mut events = EventStream::new();
        let mut input = String::new();
        let mut show_threads = false;
        let mut scroll = 0u16;
        let mut completion: Option<(String, usize)> = None;
        let mut command_selection = 0usize;
        let mut spinner = 0usize;
        let mut animation = tokio::time::interval(std::time::Duration::from_millis(80));
        animation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut changes = session.api.changes();
        let mut connected = false;
        let mut pending_refresh = false;
        let mut reported_title = String::new();
        let mut pending_enter = None;
        loop {
            runtime.observe(&session);
            runtime.poll(&session);
            if let Some(text) = runtime.next(&session) {
                active = launch(&mut session, Action::Send(text), &sender, &mut runtime);
                scroll = 0;
            }
            let state = if session.responding || runtime.active() { "working" } else if session.error.is_some() { "error" } else if session.current.is_none() { "starting" } else { "idle" };
            let identity = session.current.as_ref().map(|thread| thread.id.as_str()).or(startup_thread.as_deref()).unwrap_or("-");
            let title = format!("Solmu | {state} | {identity}");
            if title != reported_title {
                // Semantic terminal status lets Solmu muxer label each real
                // CLI pane without guessing from model-generated text.
                crossterm::execute!(io::stdout(), crossterm::terminal::SetTitle(&title))?;
                reported_title = title;
            }
            if bridge.port != 0 {
                let metadata = serde_json::to_string(&runtime.metadata(&bridge, &session))?;
                if metadata != reported_metadata {
                    write!(io::stdout(), "\x1b]777;solmu;{metadata}\x07")?;
                    io::stdout().flush()?;
                    reported_metadata = metadata;
                }
            }
            terminal.draw(|frame| if let Some(page) = &page { page.draw(frame); } else { draw(frame, &session, &input, show_threads, scroll, spinner, connected); draw_commands(frame, &input, command_selection); })?;
            let can_submit = !session.busy;
            tokio::select! {
                Some(command) = bridge.receiver.recv() => {
                    if runtime.handle(command, &session) { stop(&mut session, &mut active, &sender, &mut runtime); }
                },
                _ = animation.tick(), if session.busy => spinner = spinner.wrapping_add(1),
                Some(change) = changes.next() => {
                    match change {
                        Connection::ProfileChanged => { if let Some(page) = &mut page && page.refresh() { settings::load(session.api.clone(), settings_sender.clone()); } },
                        Connection::Disconnected => connected = false,
                        Connection::Connected | Connection::Changed => {
                            connected = true;
                            if session.busy { pending_refresh = true; } else { active = launch(&mut session, Action::Refresh, &sender, &mut runtime); }
                        }
                    }
                },
                Some(event) = settings_receiver.recv() => { if let Some(page) = &mut page { page.update(event); } },
                Some(update) = receiver.recv() => {
                    if matches!(update, Update::Opened(_, _)) { startup_thread = None; }
                    runtime.update(&update);
                    session.apply(update);
                    if !session.busy && pending_refresh { pending_refresh = false; active = launch(&mut session, Action::Refresh, &sender, &mut runtime); }
                },
                event = async {
                    if can_submit && let Some(event) = pending_enter.take() { Some(Ok(event)) }
                    else { events.next().await }
                } => {
                    let Some(event) = event else { break; };
                    let Event::Key(key) = event? else { continue; };
                    if key.kind == KeyEventKind::Release { continue; }
                    if key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        terminal.clear()?;
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'd')) { break; }
                    if let Some(current_page) = &mut page {
                        if session.busy && key.code == KeyCode::Enter && matches!(current_page, settings::Page::Models { .. }) { pending_enter = Some(Event::Key(key)); continue; }
                        match current_page.key(key) {
                            settings::Action::Close => page = None,
                            settings::Action::Save(text, model) => settings::save(session.api.clone(), text, model, settings_sender.clone()),
                            settings::Action::Model(model) => { page = None; active = launch(&mut session, Action::Model(model), &sender, &mut runtime); },
                            settings::Action::None => {},
                        }
                        continue;
                    }
                    if key.code != KeyCode::Tab { completion = None; }
                    match key.code {
                        KeyCode::Up | KeyCode::Down if !command_matches(&input).is_empty() => {
                            let count = command_matches(&input).len();
                            command_selection = if key.code == KeyCode::Up { (command_selection + count - 1) % count } else { (command_selection + 1) % count };
                        },
                        KeyCode::Esc => { stop(&mut session, &mut active, &sender, &mut runtime); input.clear(); },
                        KeyCode::Tab => {
                            let (prefix, index) = completion.get_or_insert_with(|| (input.clone(), 0));
                            if !prefix.starts_with('/') || prefix.contains(' ') { continue; }
                            let matches: Vec<_> = COMMANDS.iter().filter(|command| command.starts_with(prefix.as_str())).collect();
                            if !matches.is_empty() { input = format!("{} ", matches[*index % matches.len()]); *index += 1; }
                        },
                        KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::PageUp => scroll = scroll.saturating_add(8),
                        KeyCode::PageDown => scroll = scroll.saturating_sub(8),
                        KeyCode::Backspace => { input.pop(); command_selection = 0; },
                        KeyCode::Char(character) => { input.push(character); command_selection = 0; },
                        KeyCode::Enter => {
                            let matches = command_matches(&input);
                            if !matches.is_empty() && !COMMANDS.contains(&input.as_str()) {
                                let selected = matches[command_selection % matches.len()];
                                input = selected.into(); command_selection = 0;
                                if matches!(selected, "/open" | "/rename") { input.push(' '); continue; }
                            }
                            let text = input.trim().to_owned();
                            if text == "/exit" { break; }
                            if text == "/stop" { stop(&mut session, &mut active, &sender, &mut runtime); input.clear(); continue; }
                            if text.is_empty() { continue; }
                            if session.busy {
                                // Live refreshes and conversation operations can
                                // finish just after the terminal showed Ready.
                                // Preserve Enter until that operation completes.
                                if !session.responding { pending_enter = Some(Event::Key(key)); }
                                continue;
                            }
                            input.clear(); scroll = 0;
                            let (command, argument) = text.split_once(' ').unwrap_or((&text, ""));
                            let action = match command {
                                "/profile" => { page = Some(settings::Page::profile()); settings::load(session.api.clone(), settings_sender.clone()); continue; },
                                "/model" if argument.is_empty() => { page = Some(settings::Page::models(session.current.as_ref().and_then(|thread| thread.model.clone()))); settings::models(session.api.clone(), settings_sender.clone()); continue; },
                                "/model" => Action::Model(if argument == "default" { None } else { Some(argument.into()) }),
                                "/threads" => { show_threads = true; Action::Refresh },
                                "/new" => { show_threads = false; Action::New(if argument.is_empty() { "New conversation".into() } else { argument.into() }) },
                                "/open" if !argument.is_empty() => { show_threads = false; Action::Open(argument.into()) },
                                "/rename" if !argument.is_empty() => Action::Rename(argument.into()),
                                "/delete" => { show_threads = true; Action::Delete },
                                "/help" => { session.error = Some("/new [title] · /threads · /open <id> · /rename <title> · /model [id|default] · /profile · /delete · /stop · /exit".into()); continue; },
                                command if command.starts_with('/') => { session.error = Some("Unknown command or missing argument. Use /help.".into()); continue; },
                                _ => { show_threads = false; Action::Send(text) },
                            };
                            active = launch(&mut session, action, &sender, &mut runtime);
                        },
                        _ => {},
                    }
                }
            }
        }
        Ok::<_, io::Error>(())
    }.await;
    ratatui::restore();
    result?;
    Ok(())
}

fn draw(
    frame: &mut Frame<'_>,
    session: &Session,
    input: &str,
    show_threads: bool,
    scroll: u16,
    spinner: usize,
    connected: bool,
) {
    let embedded = std::env::var_os("SOLMU_MUXER").is_some();
    let [header, conversation, status, composer, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(if embedded { 1 } else { 3 }),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let title = session
        .current
        .as_ref()
        .map(|thread| thread.title.as_str())
        .unwrap_or("No conversation");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    " SOLMU ",
                    Style::new().fg(Color::Black).bg(Color::Cyan).bold(),
                ),
                Span::raw(format!("   {title}")),
            ]),
            Line::from(format!(
                "Model: {}  · {}",
                session
                    .current
                    .as_ref()
                    .and_then(|thread| thread.model.as_deref())
                    .unwrap_or("default"),
                session
                    .current
                    .as_ref()
                    .and_then(|thread| thread.workspace.as_deref())
                    .unwrap_or("")
            ))
            .dark_gray(),
        ])
        .block(Block::new().borders(if embedded {
            Borders::NONE
        } else {
            Borders::BOTTOM
        })),
        header,
    );
    let mut lines = Vec::new();
    if show_threads {
        lines.push(Line::from(
            "Conversations · /open <id> to continue".cyan().bold(),
        ));
        for thread in &session.threads {
            lines.push(Line::from(format!("{}  {}", thread.id, thread.title)));
        }
    } else if session.messages.is_empty() {
        lines.extend([
            Line::from(""),
            Line::from("A little space for your next big idea.".cyan().bold()),
            Line::from(""),
            Line::from("Send a message to begin. Your conversation is saved automatically."),
        ]);
    } else {
        for message in &session.messages {
            lines.push(Line::from(if message.role == "user" {
                "YOU".cyan().bold()
            } else {
                "SOLMU".green().bold()
            }));
            lines.extend(
                message
                    .content
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
            lines.push(Line::from(""));
            for tool in session
                .tools
                .iter()
                .filter(|tool| tool.message_id == message.id)
            {
                lines.push(Line::from(
                    format!("{} · {}", tool.name, tool.status).yellow().bold(),
                ));
                lines.extend(
                    tool.details()
                        .lines()
                        .map(|line| Line::from(line.to_owned())),
                );
                lines.push(Line::from(""));
            }
        }
    }
    if !session.partial.is_empty() {
        lines.push(Line::from("SOLMU · streaming".green().bold()));
        lines.extend(
            session
                .partial
                .lines()
                .map(|line| Line::from(line.to_owned())),
        );
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let content_height = paragraph.line_count(conversation.width) as u16;
    let offset = content_height
        .saturating_sub(conversation.height)
        .saturating_sub(scroll);
    frame.render_widget(paragraph.scroll((offset, 0)), conversation);
    let notice = session.error.as_deref().unwrap_or(if session.busy {
        SPINNER[spinner % SPINNER.len()]
    } else if !connected {
        "Reconnecting to live updates…"
    } else {
        "Ready"
    });
    frame.render_widget(
        Paragraph::new(notice)
            .style(Style::new().fg(if session.error.is_some() {
                Color::Red
            } else {
                Color::DarkGray
            }))
            .wrap(Wrap { trim: false }),
        status,
    );
    frame.render_widget(
        Paragraph::new(format!("> {input}")).block(
            Block::new()
                .borders(if embedded {
                    Borders::NONE
                } else {
                    Borders::ALL
                })
                .border_style(Style::new().fg(Color::Cyan)),
        ),
        composer,
    );
    frame.render_widget(
        Paragraph::new(if session.responding {
            "Esc to stop"
        } else {
            ""
        })
        .dark_gray(),
        footer,
    );
}

fn command_matches(input: &str) -> Vec<&'static str> {
    if !input.starts_with('/') || input.contains(char::is_whitespace) {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .copied()
        .filter(|command| command.starts_with(input))
        .collect()
}

fn draw_commands(frame: &mut Frame<'_>, input: &str, selected: usize) {
    let commands = command_matches(input);
    if commands.is_empty() {
        return;
    }
    let descriptions = |command| match command {
        "/new" => "Start a new conversation",
        "/threads" => "Browse saved conversations",
        "/open" => "Open a conversation by ID",
        "/model" => "Choose the model for this thread",
        "/profile" => "Edit Solmu's system prompt",
        "/rename" => "Rename this conversation",
        "/delete" => "Delete this conversation",
        "/help" => "Show available commands",
        "/stop" => "Stop the current response",
        "/exit" => "Quit Solmu",
        _ => "",
    };
    let area = frame.area();
    let height = (commands.len() as u16 + 2).min(area.height.saturating_sub(7));
    if height < 3 {
        return;
    }
    let rect = Rect::new(
        area.x + 1,
        area.y + area.height.saturating_sub(4 + height),
        area.width.saturating_sub(2).min(78),
        height,
    );
    frame.render_widget(Clear, rect);
    let lines: Vec<Line<'_>> = commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let line = format!(
                "{} {:<12} {}",
                if index == selected % commands.len() {
                    "›"
                } else {
                    " "
                },
                command,
                descriptions(command)
            );
            if index == selected % commands.len() {
                Line::from(line.green().bold())
            } else {
                Line::from(line)
            }
        })
        .collect();
    let offset = (selected as u16).saturating_sub(height.saturating_sub(3));
    frame.render_widget(
        Paragraph::new(lines).scroll((offset, 0)).block(
            Block::bordered()
                .title(" Commands ")
                .border_style(Style::new().fg(Color::DarkGray)),
        ),
        rect,
    );
}
