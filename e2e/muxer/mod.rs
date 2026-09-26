use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use solmu_e2e::support::{Backend, binary};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Terminal {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    input: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
}
impl Terminal {
    fn start(backend: &Backend, directories: &[&std::path::Path]) -> Self {
        Self::start_mode(backend, directories, true, "default")
    }
    fn start_session(backend: &Backend, name: &str) -> Self {
        Self::start_mode(backend, &[], false, name)
    }
    fn start_mode(
        backend: &Backend,
        directories: &[&std::path::Path],
        foreground: bool,
        name: &str,
    ) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 180,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(binary("muxer"));
        if foreground {
            command.arg("--foreground");
        }
        command.arg("--session");
        command.arg(name);
        command.cwd(backend.directory.path());
        command.env("SOLMU_BACKEND_URL", &backend.url);
        command.env("SOLMU_CLI_PATH", binary("solmu-cli"));
        command.env(
            "SOLMU_MUXER_DIR",
            backend.directory.path().join("muxer-state"),
        );
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env_remove("NO_COLOR");
        for directory in directories {
            command.arg("--cwd");
            command.arg(directory);
        }
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let input = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(40, 180, 0)));
        let capture = screen.clone();
        let raw = Arc::new(Mutex::new(Vec::new()));
        let bytes_capture = raw.clone();
        let writer = input.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            let mut pending = Vec::new();
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let mut raw = bytes_capture
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                raw.extend_from_slice(&bytes[..count]);
                if raw.len() > 128 * 1024 {
                    let excess = raw.len() - 128 * 1024;
                    raw.drain(..excess);
                }
                drop(raw);
                capture
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .process(&bytes[..count]);
                pending.extend_from_slice(&bytes[..count]);
                for (query, reply) in [
                    (b"\x1b[6n".as_slice(), b"\x1b[1;1R".as_slice()),
                    (b"\x1b[c".as_slice(), b"\x1b[?1;2c".as_slice()),
                    (b"\x1b[?u".as_slice(), b"\x1b[?0u".as_slice()),
                ] {
                    if let Some(position) = pending
                        .windows(query.len())
                        .position(|window| window == query)
                    {
                        let mut writer = writer.lock().unwrap();
                        let _ = writer.write_all(reply);
                        let _ = writer.flush();
                        pending.drain(..position + query.len());
                    }
                }
                if pending.len() > 32 {
                    pending.drain(..pending.len() - 32);
                }
            }
        });
        Self {
            child,
            master: pair.master,
            input,
            screen,
            raw,
        }
    }
    fn send(&self, bytes: &[u8]) {
        let mut input = self.input.lock().unwrap();
        input.write_all(bytes).unwrap();
        input.flush().unwrap();
    }
    fn command(&self, text: &str) {
        self.send(format!("{text}\r").as_bytes());
    }
    fn prefix(&self, key: char) {
        self.send(format!("\x02{key}").as_bytes());
    }
    fn contents(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }
    fn click_at(&self, column: usize, row: usize) {
        self.send(
            format!(
                "\x1b[<0;{};{}M\x1b[<0;{};{}m",
                column + 1,
                row + 1,
                column + 1,
                row + 1
            )
            .as_bytes(),
        );
    }
    async fn click(&self, label: &str) {
        self.wait(label).await;
        let position = {
            let screen = self.screen.lock().unwrap();
            let screen = screen.screen();
            let (rows, cols) = screen.size();
            (0..rows)
                .find_map(|row| {
                    let text: String = (0..cols)
                        .map(|col| {
                            let text = screen.cell(row, col).unwrap().contents();
                            if text.is_empty() { " " } else { text }
                        })
                        .collect();
                    text.find(label).map(|offset| {
                        (
                            text[..offset].chars().count() + label.chars().count() / 2,
                            usize::from(row),
                        )
                    })
                })
                .unwrap()
        };
        self.click_at(position.0, position.1);
    }
    fn close_tab(&self, id: u64) {
        let column = {
            let screen = self.screen.lock().unwrap();
            let screen = screen.screen();
            let text: String = (0..screen.size().1)
                .map(|col| {
                    let text = screen.cell(2, col).unwrap().contents();
                    if text.is_empty() { " " } else { text }
                })
                .collect();
            let start = text.find(&format!("Solmu {id}")).unwrap();
            let close = start + text[start..].find('×').unwrap();
            text[..close].chars().count()
        };
        self.click_at(column, 2);
    }
    async fn wait(&self, text: &str) {
        tokio::time::timeout(Duration::from_secs(25), async {
            while !self.contents().contains(text) {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Missing {text:?}:\n{}\nRaw output: {:?}",
                self.contents(),
                String::from_utf8_lossy(&self.raw.lock().unwrap())
            )
        });
    }
    async fn wait_absent(&self, text: &str) {
        tokio::time::timeout(Duration::from_secs(25), async {
            while self.contents().contains(text) {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Still showing {text:?}:\n{}", self.contents()));
    }
    async fn exit(&mut self) {
        self.prefix('q');
        self.wait_exit().await;
    }
    async fn wait_exit(&mut self) {
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
        self.child.wait().unwrap();
    }
    fn resize(&self, rows: u16, cols: u16) {
        self.screen
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(rows, cols);
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
    }
    async fn wait_resized(&self, cols: u16) {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                let rendered = self
                    .screen
                    .lock()
                    .unwrap()
                    .screen()
                    .rows(cols.saturating_sub(3), 3)
                    .nth(1)
                    .is_some_and(|text| text.chars().any(|ch| ('1'..='8').contains(&ch)));
                if rendered {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Missing resized toolbar:\n{}", self.contents()));
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

mod panes;

mod workspaces;

mod layouts;
mod restoration;
mod sessions;
mod startup;
mod tabs;
mod tools;
