use std::{io::{Read, Write}, sync::{Arc, Mutex}, time::Duration};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use solmu_e2e::support::{Backend, binary};

struct Terminal {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<vt100::Parser>>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}
impl Terminal {
    fn start(backend: &Backend) -> Self {
        let pair = native_pty_system().openpty(PtySize { rows: 32, cols: 110, pixel_width: 0, pixel_height: 0 }).unwrap();
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
                if count == 0 { break; }
                capture.lock().unwrap_or_else(|error| error.into_inner()).process(&buffer[..count]);
                pending.extend_from_slice(&buffer[..count]);
                for (query, reply) in [(b"\x1b[6n".as_slice(), b"\x1b[1;1R".as_slice()), (b"\x1b[c".as_slice(), b"\x1b[?1;2c".as_slice()), (b"\x1b[?u".as_slice(), b"\x1b[?0u".as_slice())] {
                    if pending.windows(query.len()).any(|window| window == query) {
                        let mut input = input.lock().unwrap();
                        input.write_all(reply).unwrap(); input.flush().unwrap();
                    }
                }
                if pending.len() > 32 { pending.drain(..pending.len()-32); }
            }
        });
        Self { child, writer, screen, _master: pair.master }
    }
    fn command(&mut self, text: &str) { let mut writer = self.writer.lock().unwrap(); writer.write_all(format!("{text}\r").as_bytes()).unwrap(); writer.flush().unwrap(); }
    async fn wait(&self, expected: &str) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if self.screen.lock().unwrap().screen().contents().contains(expected) { break; }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        }).await.unwrap_or_else(|_| panic!("Missing {expected:?} in terminal:\n{}", self.screen.lock().unwrap().screen().contents()));
    }
    async fn ready(&self) { self.wait("Ready ·").await; }
    async fn exit(&mut self) {
        self.command("/exit");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop { if let Some(status) = self.child.try_wait().unwrap() { assert!(status.success()); break; } tokio::time::sleep(Duration::from_millis(30)).await; }
        }).await.unwrap();
    }
}
impl Drop for Terminal { fn drop(&mut self) { let _ = self.child.kill(); } }

#[tokio::test(flavor = "multi_thread")]
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
    assert_eq!(backend.messages(&id).await["items"].as_array().unwrap().len(), 1);
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert_eq!(backend.messages(&id).await["items"][1]["content"], "Hello from Solmu");
    terminal.command("/rename Terminal planning"); terminal.wait("Terminal planning").await; terminal.ready().await;
    terminal.command("/threads"); terminal.wait("Conversations ·").await; terminal.wait(&id).await; terminal.ready().await;
    terminal.command("/new Another idea"); terminal.wait("Another idea").await; terminal.ready().await;
    assert_eq!(backend.threads().await["items"].as_array().unwrap().len(), 2);
    terminal.command(&format!("/open {id}")); terminal.wait("Hello from the terminal").await; terminal.ready().await;
    terminal.command("/delete"); terminal.wait("No conversation").await; terminal.ready().await;
    assert_eq!(backend.threads().await["items"].as_array().unwrap().len(), 1);
    terminal.command("/help"); terminal.wait("/rename <title>").await;
    terminal.command("/unknown"); terminal.wait("Unknown command").await;
    terminal.exit().await;
    let mut again = Terminal::start(&backend);
    again.ready().await;
    assert_eq!(backend.threads().await["items"].as_array().unwrap().len(), 2);
    again.exit().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_shows_provider_and_membership_errors_and_can_exit_while_streaming() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("FAIL"); terminal.wait("LLM request failed").await;
    terminal.command("/open missing"); terminal.wait("not found").await;
    terminal.command("/new Recovery"); terminal.wait("Recovery").await; terminal.ready().await;
    terminal.command("Try again"); terminal.wait("SOLMU · streaming").await;
    terminal.exit().await;
}
