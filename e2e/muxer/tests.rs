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
}
impl Terminal {
    fn start(backend: &Backend, directories: &[&std::path::Path]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 180,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(binary("muxer"));
        command.cwd(backend.directory.path());
        command.env("SOLMU_BACKEND_URL", &backend.url);
        command.env("SOLMU_CLI_PATH", binary("solmu-cli"));
        command.env("TERM", "xterm-256color");
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
        let writer = input.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            let mut pending = Vec::new();
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
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
    async fn wait(&self, text: &str) {
        tokio::time::timeout(Duration::from_secs(25), async {
            while !self.contents().contains(text) {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Missing {text:?}:\n{}", self.contents()));
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
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_solmu_panes_stream_switch_split_resize_and_keep_conversation_features() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready ·").await;
    tui.command("/rename First workspace");
    tui.wait("First workspace").await;
    tui.wait("Ready ·").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Hello from pane one");
    tui.wait("Solmu 1 · working").await;
    tui.wait("SOLMU · streaming").await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("/ren\tSecond workspace");
    tui.wait("Second workspace").await;
    tui.wait("Ready ·").await;
    tui.prefix('1');
    tui.wait("Hello from Solmu").await;
    tui.wait("Ready ·").await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    tui.prefix('s');
    tui.wait("Second workspace").await;
    tui.wait("First workspace").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer");
    tui.resize(44, 200);
    tui.wait("First workspace").await;
    tui.wait("Second workspace").await;
    // Click the second sidebar entry; keystrokes must reach that pane only.
    tui.send(b"\x1b[<0;3;6M\x1b[<0;3;6m");
    tui.command("/rename Clicked workspace");
    tui.wait("Clicked workspace").await;
    tui.prefix('s');
    tui.command(&format!("/open {id}"));
    tui.wait("Hello from pane one").await;
    tui.wait("Ready ·").await;
    tui.command("/threads");
    tui.wait("Conversations ·").await;
    tui.wait(&id).await;
    tui.wait("Ready ·").await;
    tui.command("/new Third conversation");
    tui.wait("Third conversation").await;
    tui.wait("Ready ·").await;
    tui.command("/delete");
    tui.wait("No conversation").await;
    tui.wait("Ready ·").await;
    tui.command("/exit");
    tui.wait("Solmu 2 · exited 0").await;
    tui.prefix('r');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.prefix('x');
    tui.wait("First workspace").await;
    assert!(!tui.contents().contains("Solmu 2 ·"));
    tui.exit().await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_selection_validation_error_state_and_stopping_replies() {
    let backend = Backend::start().await;
    let other = backend.directory.path().join("project with spaces");
    std::fs::create_dir(&other).unwrap();
    let mut tui = Terminal::start(&backend, &[backend.directory.path(), &other]);
    tui.wait("Solmu 1 · idle").await;
    tui.prefix(']');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.prefix('w');
    tui.wait("Workspace path:").await;
    tui.command("solmu-missing-workspace");
    tui.wait("Workspace path: solmu-missing-workspace").await;
    assert!(tui.contents().contains("Workspace path:"));
    tui.send(b"\x1b");
    // Unix terminals encode Alt using an Escape prefix. Confirm cancellation
    // before sending Ctrl+b, so separate user actions cannot become Alt+Ctrl+b.
    tui.wait_absent("New workspace").await;
    tui.prefix('w');
    tui.command(other.to_str().unwrap());
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("FAIL_STREAM");
    tui.wait("Solmu 3 · error").await;
    tui.command("/new Stop conversation");
    tui.wait("Stop conversation").await;
    tui.wait("Ready ·").await;
    tui.command("Hello to stop");
    tui.wait("SOLMU · streaming").await;
    tui.command("/stop");
    tui.wait("Ready ·").await;
    let threads = backend.threads().await;
    let id = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Stop conversation")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    tui.prefix('[');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("/rename Quit cleanup");
    tui.wait("Quit cleanup").await;
    tui.wait("Ready ·").await;
    let threads = backend.threads().await;
    let cleanup_id = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Quit cleanup")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Reply interrupted by muxer exit");
    tui.wait("SOLMU · streaming").await;
    tui.exit().await;
    // Owned CLIs must have exited: no further new conversations appear.
    let count = backend.threads().await["items"].as_array().unwrap().len();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        count
    );
    assert_eq!(
        backend.messages(&cleanup_id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "Muxer exit must terminate its CLI and abandon the pending assistant reply"
    );
}

#[test]
fn muxer_help_version_and_invalid_startup() {
    let directory = tempfile::tempdir().unwrap();
    let mut too_many = std::process::Command::new(binary("muxer"));
    for _ in 0..9 {
        too_many.arg("--cwd").arg(directory.path());
    }
    let output = too_many.output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("eight panes"));
    for option in ["--help", "--version"] {
        let output = std::process::Command::new(binary("muxer"))
            .arg(option)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Solmu muxer"));
    }
    for args in [
        vec!["--unknown"],
        vec!["--cwd"],
        vec!["--cwd", "solmu-missing-workspace"],
    ] {
        assert!(
            !std::process::Command::new(binary("muxer"))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
