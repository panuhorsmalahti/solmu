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
        Self::start_in(backend, false)
    }
    fn start_in(backend: &Backend, isolated: bool) -> Self {
        Self::start_with_options(backend, isolated, None)
    }
    fn start_with_thread(backend: &Backend, thread: &str) -> Self {
        Self::start_with_options(backend, false, Some(thread))
    }
    fn start_with_options(backend: &Backend, isolated: bool, thread: Option<&str>) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 32,
                cols: 110,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(binary(if isolated { "boxer" } else { "solmu" }));
        if let Some(thread) = thread {
            command.arg("--thread");
            command.arg(thread);
        }
        if isolated {
            command.arg("--isolated");
            command.arg("--cwd");
            command.arg(backend.directory.path());
            command.arg("--");
            command.arg(binary("solmu"));
        }
        command.cwd(backend.directory.path());
        command.env("SOLMU_BACKEND_URL", &backend.url);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env_remove("NO_COLOR");
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
        self.wait("Ready").await;
    }
    fn screenshot_source(&self) {
        solmu_e2e::support::capture_terminal(self.screen.lock().unwrap().screen(), "cli");
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

mod conversations;

mod responses;

mod input;
mod models;
mod profile;
mod startup;
mod tools;

#[cfg(target_os = "linux")]
mod isolation;
