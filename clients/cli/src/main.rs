use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use solmu_client::{Action, Api, Connection, Session, Update};
use std::{error::Error, io};
use tokio::sync::mpsc;

const COMMANDS: &[&str] = &[
    "/delete", "/exit", "/help", "/new", "/open", "/rename", "/stop", "/threads",
];
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn main() -> Result<(), Box<dyn Error>> {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env".into());
    }
    run()
}

fn launch(
    session: &mut Session,
    action: Action,
    sender: &mpsc::UnboundedSender<Update>,
) -> Option<tokio::task::JoinHandle<()>> {
    let updates = if matches!(action, Action::Refresh) {
        session.refresh()
    } else {
        session.begin(action)
    };
    if let Some(mut updates) = updates {
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
) {
    if !session.responding {
        return;
    }
    if let Some(task) = active.take() {
        task.abort();
    }
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
        let update = match api.stop(&id).await {
            Ok(()) => Update::Stopped,
            Err(error) => Update::Failed(error),
        };
        let _ = sender.send(update);
    }));
}

#[tokio::main(worker_threads = 2)]
async fn run() -> Result<(), Box<dyn Error>> {
    let mut terminal = ratatui::init();
    let result = async {
        let mut session = Session::new(Api::from_env());
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut active = launch(&mut session, Action::New("New conversation".into()), &sender);
        let mut events = EventStream::new();
        let mut input = String::new();
        let mut show_threads = false;
        let mut scroll = 0u16;
        let mut completion: Option<(String, usize)> = None;
        let mut spinner = 0usize;
        let mut animation = tokio::time::interval(std::time::Duration::from_millis(80));
        animation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut changes = session.api.changes();
        let mut connected = false;
        let mut pending_refresh = false;
        let mut reported_state = "";
        let mut pending_enter = None;
        loop {
            let state = if session.error.is_some() { "error" } else if session.busy { "working" } else { "idle" };
            if state != reported_state {
                // Semantic terminal status lets Solmu muxer label each real
                // CLI pane without guessing from model-generated text.
                crossterm::execute!(io::stdout(), crossterm::terminal::SetTitle(format!("Solmu | {state}")))?;
                reported_state = state;
            }
            terminal.draw(|frame| draw(frame, &session, &input, show_threads, scroll, spinner, connected))?;
            let can_submit = !session.busy;
            tokio::select! {
                _ = animation.tick(), if session.busy => spinner = spinner.wrapping_add(1),
                Some(change) = changes.next() => {
                    match change {
                        Connection::Disconnected => connected = false,
                        Connection::Connected | Connection::Changed => {
                            connected = true;
                            if session.busy { pending_refresh = true; } else { active = launch(&mut session, Action::Refresh, &sender); }
                        }
                    }
                },
                Some(update) = receiver.recv() => {
                    session.apply(update);
                    if !session.busy && pending_refresh { pending_refresh = false; active = launch(&mut session, Action::Refresh, &sender); }
                },
                event = async {
                    if can_submit && let Some(event) = pending_enter.take() { Some(Ok(event)) }
                    else { events.next().await }
                } => {
                    let Some(event) = event else { break; };
                    let Event::Key(key) = event? else { continue; };
                    if key.kind == KeyEventKind::Release { continue; }
                    if key.code != KeyCode::Tab { completion = None; }
                    match key.code {
                        KeyCode::Esc => { stop(&mut session, &mut active, &sender); input.clear(); },
                        KeyCode::Tab => {
                            let (prefix, index) = completion.get_or_insert_with(|| (input.clone(), 0));
                            if !prefix.starts_with('/') || prefix.contains(' ') { continue; }
                            let matches: Vec<_> = COMMANDS.iter().filter(|command| command.starts_with(prefix.as_str())).collect();
                            if !matches.is_empty() { input = format!("{} ", matches[*index % matches.len()]); *index += 1; }
                        },
                        KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::PageUp => scroll = scroll.saturating_add(8),
                        KeyCode::PageDown => scroll = scroll.saturating_sub(8),
                        KeyCode::Backspace => { input.pop(); },
                        KeyCode::Char(character) => input.push(character),
                        KeyCode::Enter => {
                            let text = input.trim().to_owned();
                            if text == "/exit" { break; }
                            if text == "/stop" { stop(&mut session, &mut active, &sender); input.clear(); continue; }
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
                                "/threads" => { show_threads = true; Action::Refresh },
                                "/new" => { show_threads = false; Action::New(if argument.is_empty() { "New conversation".into() } else { argument.into() }) },
                                "/open" if !argument.is_empty() => { show_threads = false; Action::Open(argument.into()) },
                                "/rename" if !argument.is_empty() => Action::Rename(argument.into()),
                                "/delete" => { show_threads = true; Action::Delete },
                                "/help" => { session.error = Some("/new [title] · /threads · /open <id> · /rename <title> · /delete · /stop · /exit".into()); continue; },
                                command if command.starts_with('/') => { session.error = Some("Unknown command or missing argument. Use /help.".into()); continue; },
                                _ => { show_threads = false; Action::Send(text) },
                            };
                            active = launch(&mut session, action, &sender);
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
    let [header, conversation, status, composer, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let title = session
        .current
        .as_ref()
        .map(|thread| thread.title.as_str())
        .unwrap_or("No conversation");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " SOLMU ",
                Style::new().fg(Color::Black).bg(Color::Cyan).bold(),
            ),
            Span::raw(format!("   {title}")),
        ]))
        .block(Block::new().borders(Borders::BOTTOM)),
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
            Line::from("Use /help for conversation commands."),
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
        "Ready · conversation saved locally"
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
        Paragraph::new(format!("> {input}"))
            .block(Block::bordered().border_style(Style::new().fg(Color::Cyan))),
        composer,
    );
    frame.render_widget(
        Paragraph::new(if session.responding {
            "Esc stop   /stop stop reply   /exit quit"
        } else {
            "Enter send   Tab complete   /help commands   PageUp/PageDown scroll   /exit quit"
        })
        .dark_gray(),
        footer,
    );
}
