mod agent;
mod app;
mod bindings;
mod config;
mod control;
mod editor;
mod keys;
mod layout;
mod monitor;
mod pane;
mod persistence;
mod session;
mod terminal;
#[cfg(windows)]
mod windows;

use app::{App, pane_inner};
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use std::{error::Error, io, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    if control::cli(std::env::args_os().skip(1).collect()) {
        return Ok(());
    }
    let mut directories = Vec::new();
    let mut mode = "attach".to_owned();
    let mut name = "default".to_owned();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Solmu muxer\n\nUsage: muxer [--session NAME] [--cwd PATH]... [--foreground]\n       muxer session list\n       muxer session attach NAME\n       muxer server start|status|stop [--session NAME]\n       muxer pane read ID [--session NAME]\n\nSpaces group Solmu tabs by working directory. Tabs hold real terminal panes.\nStart solmu-backend first. Click spaces, tabs, close icons, or toolbar controls.\nRight-click spaces, tabs, or panes for actions; drag dividers to resize.\nCtrl+b then: n new tab; w new space; Tab/] next tab; [ previous tab;\nUp/Down switch space; 1-8 select tab; s/v split right; - split down;\nh/j/k/l focus; H/J/K/L swap; z zoom; r resize/restart; x close pane;\nX close tab; D close space; W/T/P rename space/tab/pane; g find; m navigate;\n? help; comma settings; B toggle sidebar; q detach. Ctrl+b b sends literal Ctrl+b.\nUp to eight spaces, eight tabs per space, and eight panes per tab.\nSOLMU_BACKEND_URL selects the backend; SOLMU_CLI_PATH selects solmu.\nDefault: attach to a local background session; panes survive detaching.\nUse server stop to terminate panes, or --foreground for a temporary session.\nSOLMU_MUXER_DIR selects the session state directory.\nEdit config.toml there for automatically applied shortcuts, colors, and settings.\nSOLMU_MUXER_CONFIG selects another file; --default-config prints defaults."
            );
            println!("\n{}", control::HELP);
            return Ok(());
        } else if arg == "--version" {
            println!("Solmu muxer {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        } else if arg == "--default-config" {
            print!("{}", config::default_text());
            return Ok(());
        } else if arg == "--cwd" {
            directories.push(PathBuf::from(
                args.next().ok_or("--cwd requires a directory")?,
            ));
        } else if arg == "--session" || arg == "--server" {
            if arg == "--server" {
                mode = "serve".into();
            }
            name = args
                .next()
                .ok_or("A session name is required")?
                .into_string()
                .map_err(|_| "Invalid session name")?;
        } else if arg == "--foreground" {
            mode = "foreground".into();
        } else if arg == "server" {
            mode = args
                .next()
                .ok_or("Use server start, status, or stop")?
                .into_string()
                .map_err(|_| "Invalid server command")?;
            if !matches!(mode.as_str(), "start" | "status" | "stop") {
                return Err("Use server start, status, or stop".into());
            }
        } else if arg == "session" {
            let action = args.next().ok_or("Use session list or attach NAME")?;
            if action == "list" {
                mode = "list".into();
            } else if action == "attach" {
                name = args
                    .next()
                    .ok_or("Session name is required")?
                    .into_string()
                    .map_err(|_| "Invalid session name")?;
            } else {
                return Err("Use session list or attach NAME".into());
            }
        } else if arg == "pane" {
            if args.next().is_none_or(|arg| arg != "read") {
                return Err("Use pane read ID".into());
            }
            let id = args
                .next()
                .ok_or("Pane ID is required")?
                .into_string()
                .map_err(|_| "Invalid pane ID")?
                .parse::<u64>()?;
            mode = format!("read:{id}");
        } else {
            return Err("Unknown option; use muxer --help".into());
        }
    }
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env".into());
    }
    session::validate_name(&name)?;
    if mode.starts_with("read:") {
        return session::control(&name, &mode);
    }
    match mode.as_str() {
        "list" => return session::list(),
        "stop" | "status" => return session::control(&name, &mode),
        _ => {}
    }
    if directories.len() > 8 {
        return Err("Solmu muxer supports up to eight spaces".into());
    }
    if mode != "foreground" && session::has_session(&name)? {
        // Live and restored layouts own their directories. A changed new-pane
        // policy must not prevent reattaching to existing work.
        directories = vec![std::env::current_dir()?];
    } else if directories.is_empty() {
        let config = config::Manager::new()?;
        directories.push(
            config
                .current
                .cwd(&std::env::current_dir()?, config.path.parent().unwrap())?,
        );
    }
    for directory in &mut directories {
        *directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err("Workspace must be a directory".into());
        }
    }
    match mode.as_str() {
        "attach" => return session::attach(&name, &directories),
        "start" => {
            session::ensure(&name, &directories)?;
            println!("Started Muxer session {name}");
            return Ok(());
        }
        _ => {}
    }
    let executable = if let Some(path) = std::env::var_os("SOLMU_CLI_PATH") {
        PathBuf::from(path).canonicalize()?
    } else {
        let sibling = std::env::current_exe()?
            .with_file_name(format!("solmu{}", std::env::consts::EXE_SUFFIX));
        if sibling.is_file() {
            sibling
        } else {
            PathBuf::from(format!("solmu{}", std::env::consts::EXE_SUFFIX))
        }
    };
    if mode == "serve" {
        return session::serve(&name, executable, directories);
    }
    let mut app = App::new(executable)?;
    for directory in directories {
        app.add_space(directory)?;
    }
    app.select_space(0);
    let mut terminal = ratatui::init();
    let result = (|| -> Result<(), Box<dyn Error>> {
        crossterm::execute!(
            io::stdout(),
            event::EnableMouseCapture,
            event::EnableBracketedPaste
        )?;
        loop {
            app.config.refresh(false);
            for pane in &mut app.panes {
                pane.poll()?;
            }
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            app.resized(area);
            for (index, rect) in app.visible(area) {
                app.panes[index].resize(pane_inner(rect))?;
            }
            terminal.draw(|frame| app.draw(frame))?;
            if !event::poll(Duration::from_millis(40))? {
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if app.key(key)? {
                        break;
                    }
                }
                Event::Paste(text) => {
                    app.paste(&text)?;
                }
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                        && app.click(area, mouse.column, mouse.row)? =>
                {
                    break;
                }
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) =>
                {
                    app.context_menu(area, mouse.column, mouse.row)
                }
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Drag(MouseButton::Left)) =>
                {
                    app.drag(mouse.column, mouse.row)
                }
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) =>
                {
                    app.end_drag()
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = crossterm::execute!(
        io::stdout(),
        event::DisableMouseCapture,
        event::DisableBracketedPaste
    );
    ratatui::restore();
    drop(app); // Kill and reap every owned CLI when the foreground muxer exits.
    result
}
