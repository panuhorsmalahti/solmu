mod app;
mod keys;
mod layout;
mod pane;

use app::{App, pane_inner};
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use std::{error::Error, io, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let mut directories = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Solmu muxer\n\nUsage: muxer [--cwd PATH]...\n\nSpaces group Solmu tabs by working directory. Tabs hold real terminal panes.\nStart solmu-backend first. Click spaces, tabs, close icons, or toolbar controls.\nRight-click a pane for actions; drag dividers to resize.\nCtrl+b then: n new tab; w new space; Tab/] next tab; [ previous tab;\nUp/Down switch space; 1-8 select tab; s/v split right; - split down;\nh/j/k/l focus; H/J/K/L swap; z zoom; r resize/restart; x close pane;\nX close tab; q quit. Ctrl+b b sends literal Ctrl+b.\nUp to eight spaces, eight tabs per space, and eight panes per tab.\nSOLMU_BACKEND_URL selects the backend; SOLMU_CLI_PATH selects solmu-cli.\nForeground sessions only: quitting terminates the launched CLIs."
            );
            return Ok(());
        } else if arg == "--version" {
            println!("Solmu muxer {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        } else if arg == "--cwd" {
            directories.push(PathBuf::from(
                args.next().ok_or("--cwd requires a directory")?,
            ));
        } else {
            return Err("Unknown option; use muxer --help".into());
        }
    }
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env".into());
    }
    if directories.is_empty() {
        directories.push(std::env::current_dir()?);
    }
    if directories.len() > 8 {
        return Err("Solmu muxer supports up to eight spaces".into());
    }
    let executable = if let Some(path) = std::env::var_os("SOLMU_CLI_PATH") {
        PathBuf::from(path).canonicalize()?
    } else {
        let sibling = std::env::current_exe()?
            .with_file_name(format!("solmu-cli{}", std::env::consts::EXE_SUFFIX));
        if sibling.is_file() {
            sibling
        } else {
            PathBuf::from(format!("solmu-cli{}", std::env::consts::EXE_SUFFIX))
        }
    };
    let mut app = App::new(executable);
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
                Event::Paste(text) if app.workspace.is_some() => {
                    if let Some(path) = &mut app.workspace {
                        path.push_str(&text.replace(['\r', '\n'], ""));
                    }
                }
                Event::Paste(text) => {
                    app.panes[app.active].send(text.as_bytes())?;
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
