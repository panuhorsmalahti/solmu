use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use solmu_e2e::support::{Backend, binary};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Terminal {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<vt100::Parser>>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}
impl Terminal {
    fn start(backend: &Backend) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 32,
                cols: 110,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(binary("solmu-cli"));
        command.cwd(backend.directory.path());
        command.env("SOLMU_BACKEND_URL", &backend.url);
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(32, 110, 0)));
        let capture = screen.clone();
        let input = writer.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            let mut pending = Vec::new();
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                capture
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .process(&buffer[..count]);
                pending.extend_from_slice(&buffer[..count]);
                for (query, reply) in [
                    (b"\x1b[6n".as_slice(), b"\x1b[1;1R".as_slice()),
                    (b"\x1b[c".as_slice(), b"\x1b[?1;2c".as_slice()),
                    (b"\x1b[?u".as_slice(), b"\x1b[?0u".as_slice()),
                ] {
                    if pending.windows(query.len()).any(|window| window == query) {
                        let mut input = input.lock().unwrap();
                        input.write_all(reply).unwrap();
                        input.flush().unwrap();
                    }
                }
                if pending.len() > 32 {
                    pending.drain(..pending.len() - 32);
                }
            }
        });
        Self {
            child,
            writer,
            screen,
            _master: pair.master,
        }
    }
    fn command(&mut self, text: &str) {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(format!("{text}\r").as_bytes()).unwrap();
        writer.flush().unwrap();
    }
    async fn wait(&self, expected: &str) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if self
                    .screen
                    .lock()
                    .unwrap()
                    .screen()
                    .contents()
                    .contains(expected)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Missing {expected:?} in terminal:\n{}",
                self.screen.lock().unwrap().screen().contents()
            )
        });
    }
    async fn ready(&self) {
        self.wait("Ready ·").await;
    }
    fn screenshot_source(&self) {
        if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_none() {
            return;
        }
        fn color(value: vt100::Color) -> String {
            match value {
                vt100::Color::Default => "inherit".into(),
                vt100::Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
                vt100::Color::Idx(index) => {
                    const PALETTE: [&str; 16] = [
                        "#151a23", "#eb6f92", "#9ccfd8", "#f6c177", "#719cd6", "#c4a7e7",
                        "#63cdcf", "#e0def4", "#747986", "#eb6f92", "#9ccfd8", "#f6c177",
                        "#91b4df", "#c4a7e7", "#9be7e8", "#ffffff",
                    ];
                    if index < 16 {
                        PALETTE[index as usize].into()
                    } else if index < 232 {
                        let value = index - 16;
                        let component = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                        format!(
                            "rgb({},{},{})",
                            component(value / 36),
                            component(value / 6 % 6),
                            component(value % 6)
                        )
                    } else {
                        let value = 8 + (index - 232) * 10;
                        format!("rgb({value},{value},{value})")
                    }
                }
            }
        }
        let parser = self.screen.lock().unwrap();
        let screen = parser.screen();
        let mut html = String::from(
            "<!doctype html><meta charset=utf-8><style>body{margin:0;background:#151a23;color:#e0def4;padding:24px}pre{margin:0;font:14px/20px Consolas,'DejaVu Sans Mono',monospace}span{display:inline-block;width:1ch;height:20px}</style><pre>",
        );
        let (rows, columns) = screen.size();
        for row in 0..rows {
            for column in 0..columns {
                let cell = screen.cell(row, column).unwrap();
                if cell.is_wide_continuation() {
                    continue;
                }
                let contents = cell.contents();
                let contents = if contents.is_empty() {
                    " ".into()
                } else {
                    contents
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                };
                html.push_str(&format!("<span style=\"color:{};background:{};font-weight:{};width:{}ch\">{contents}</span>", color(cell.fgcolor()), color(cell.bgcolor()), if cell.bold() { "700" } else { "400" }, if cell.is_wide() { 2 } else { 1 }));
            }
            html.push('\n');
        }
        html.push_str("</pre>");
        let directory = solmu_e2e::support::root().join("artifacts");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("cli.html"), html).unwrap();
    }
    async fn exit(&mut self) {
        self.command("/exit");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_streaming_thread_commands_history_and_exit() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let threads = backend.threads().await;
    assert_eq!(threads["items"].as_array().unwrap().len(), 1);
    let id = threads["items"][0]["id"].as_str().unwrap().to_owned();
    terminal.command("Hello from the terminal");
    terminal.wait("SOLMU · streaming").await;
    terminal.wait("Hello").await;
    assert!(
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            .iter()
            .any(|frame| terminal
                .screen
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains(frame)),
        "Busy state must show a spinner"
    );
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert_eq!(
        backend.messages(&id).await["items"][1]["content"],
        "Hello from Solmu"
    );
    terminal.command("/ren\tTerminal planning");
    terminal.wait("Terminal planning").await;
    terminal.ready().await;
    backend
        .client
        .patch(format!("{}/api/v1/threads/{id}", backend.url))
        .json(&serde_json::json!({"title": "Across clients"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    terminal.wait("Across clients").await;
    terminal.ready().await;
    terminal.command("/rename Terminal planning");
    terminal.wait("Terminal planning").await;
    terminal.ready().await;
    terminal.screenshot_source();
    terminal.command("/threads");
    terminal.wait("Conversations ·").await;
    terminal.wait(&id).await;
    terminal.ready().await;
    terminal.command("/new Another idea");
    terminal.wait("Another idea").await;
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    terminal.command(&format!("/open {id}"));
    terminal.wait("Hello from the terminal").await;
    terminal.ready().await;
    terminal.command("/delete");
    terminal.wait("No conversation").await;
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    terminal.command("/help");
    terminal.wait("/rename <title>").await;
    terminal.command("/unknown");
    terminal.wait("Unknown command").await;
    terminal.exit().await;
    let mut again = Terminal::start(&backend);
    again.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    again.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_shows_provider_and_membership_errors_and_can_exit_while_streaming() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("FAIL");
    terminal.wait("LLM request failed").await;
    terminal.command("/open missing");
    terminal.wait("not found").await;
    terminal.command("/new Recovery");
    terminal.wait("Recovery").await;
    terminal.ready().await;
    terminal.command("Try again");
    terminal.wait("SOLMU · streaming").await;
    terminal.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_stop_command_and_escape_cancel_reply_and_allow_continuing() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (index, stop) in ["/stop", "\x1b"].iter().enumerate() {
        terminal.command("A response to stop");
        terminal.wait("SOLMU · streaming").await;
        terminal.wait("Hello").await;
        if *stop == "\x1b" {
            let mut writer = terminal.writer.lock().unwrap();
            writer.write_all(stop.as_bytes()).unwrap();
            writer.flush().unwrap();
        } else {
            terminal.command(stop);
        }
        terminal.ready().await;
        assert_eq!(
            backend.messages(&id).await["items"]
                .as_array()
                .unwrap()
                .len(),
            index + 1
        );
        assert!(
            !terminal
                .screen
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains("SOLMU · streaming")
        );
    }
    terminal.command("Continue after stopping");
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    terminal.exit().await;
}
