use crate::app::{App, View, pane_inner};
use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind, MouseButton, MouseEventKind},
    terminal,
};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    error::Error,
    fs::{self, File},
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PROTOCOL: u32 = 1;
const MAX_PACKET: usize = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Endpoint {
    protocol: u32,
    port: u16,
    token: String,
    pid: u32,
}
#[derive(Serialize, Deserialize)]
enum Request {
    Hello {
        protocol: u32,
        token: String,
        operation: String,
        width: u16,
        height: u16,
    },
    Input(Event),
}
#[derive(Serialize, Deserialize)]
enum Response {
    Text(String),
    Ready { pid: u32 },
    Frame(Vec<u8>),
    Detached,
    Error(String),
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || b"_-".contains(&ch))
    {
        return Err(
            "Session names must contain 1–64 letters, digits, underscores, or hyphens".into(),
        );
    }
    Ok(())
}
pub fn root() -> Result<PathBuf> {
    let directory = match std::env::var_os("SOLMU_MUXER_DIR") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .ok_or("Set SOLMU_MUXER_DIR when a home directory is unavailable")?,
        )
        .join(".solmu")
        .join("muxer"),
    };
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory.canonicalize()?)
}
fn private_file(path: &Path, append: bool) -> io::Result<File> {
    let mut options = File::options();
    options
        .create(true)
        .read(true)
        .write(true)
        .append(append)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn packet<T: Serialize>(stream: &mut TcpStream, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_PACKET {
        return Err(io::Error::other("Muxer packet is too large"));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)
}
fn receive<T: DeserializeOwned>(stream: &mut TcpStream) -> io::Result<T> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > MAX_PACKET {
        return Err(io::Error::other("Muxer packet is too large"));
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn connect(directory: &Path, name: &str, operation: &str, size: (u16, u16)) -> Result<TcpStream> {
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(directory.join(format!("{name}.endpoint")))?)?;
    if endpoint.protocol != PROTOCOL {
        return Err("Muxer server protocol changed; stop the server before restarting".into());
    }
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", endpoint.port).parse()?,
        Duration::from_secs(1),
    )?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    packet(
        &mut stream,
        &Request::Hello {
            protocol: PROTOCOL,
            token: endpoint.token,
            operation: operation.into(),
            width: size.0,
            height: size.1,
        },
    )?;
    match receive(&mut stream)? {
        Response::Ready { .. } => {
            stream.set_read_timeout(None)?;
            Ok(stream)
        }
        Response::Error(error) => Err(error.into()),
        _ => Err("Unexpected Muxer handshake".into()),
    }
}
#[cfg(windows)]
fn spawn(directory: &Path, name: &str, directories: &[PathBuf]) -> Result<()> {
    crate::windows::spawn(directory, name, directories)?;
    Ok(())
}
#[cfg(unix)]
fn spawn(directory: &Path, name: &str, directories: &[PathBuf]) -> Result<()> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--server")
        .arg(name)
        .env("SOLMU_MUXER_DIR", directory);
    command.env("TERM", "xterm-256color");
    for cwd in directories {
        command.arg("--cwd").arg(cwd);
    }
    let log = private_file(&directory.join(format!("{name}.log")), true)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe extern "C" {
            fn setsid() -> i32;
        }
        // setsid is async-signal-safe and disconnects the background server
        // from the launching terminal's process group and controlling TTY.
        unsafe {
            command.pre_exec(|| {
                if setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    command.spawn()?;
    Ok(())
}
pub fn ensure(name: &str, directories: &[PathBuf]) -> Result<PathBuf> {
    validate_name(name)?;
    let directory = root()?;
    if connect(&directory, name, "status", (80, 24)).is_ok() {
        return Ok(directory);
    }
    spawn(&directory, name, directories)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if connect(&directory, name, "status", (80, 24)).is_ok() {
            return Ok(directory);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "Could not start session {name}; see {}",
        directory.join(format!("{name}.log")).display()
    )
    .into())
}
pub fn control(name: &str, operation: &str) -> Result<()> {
    validate_name(name)?;
    let directory = root()?;
    let mut stream = connect(&directory, name, operation, (80, 24))?;
    if operation.starts_with("read:") {
        return match receive(&mut stream)? {
            Response::Text(text) => {
                println!("{text}");
                Ok(())
            }
            Response::Error(error) => Err(error.into()),
            _ => Err("Unexpected pane response".into()),
        };
    }
    if operation == "stop" {
        let deadline = Instant::now() + Duration::from_secs(5);
        while directory.join(format!("{name}.endpoint")).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if directory.join(format!("{name}.endpoint")).exists() {
            return Err("Muxer server did not finish stopping".into());
        }
        println!("Stopped Muxer session {name}");
    } else {
        println!("Muxer session {name} is running");
    }
    Ok(())
}
pub fn list() -> Result<()> {
    let directory = root()?;
    let mut names: Vec<_> = fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "endpoint"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(String::from)
        })
        .collect();
    names.sort();
    for name in names {
        if connect(&directory, &name, "status", (80, 24)).is_ok() {
            println!("{name}\trunning");
        }
    }
    Ok(())
}

pub fn attach(name: &str, directories: &[PathBuf]) -> Result<()> {
    let directory = ensure(name, directories)?;
    let size = terminal::size()?;
    let mut stream = connect(&directory, name, "attach", size)?;
    let mut reader = stream.try_clone()?;
    let (sender, receiver) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        while let Ok(response) = receive::<Response>(&mut reader) {
            let ended = matches!(response, Response::Detached | Response::Error(_));
            if sender.send(response).is_err() || ended {
                break;
            }
        }
    });
    terminal::enable_raw_mode()?;
    let result = (|| -> Result<()> {
        crossterm::execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            event::EnableMouseCapture,
            event::EnableBracketedPaste,
            cursor::Hide
        )?;
        loop {
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(Response::Frame(bytes)) => {
                    let mut output = io::stdout().lock();
                    output.write_all(&bytes)?;
                    output.flush()?;
                }
                Ok(Response::Detached) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Response::Error(error)) => return Err(error.into()),
                _ => {}
            }
            if event::poll(Duration::ZERO)? {
                packet(&mut stream, &Request::Input(event::read()?))?;
            }
        }
        Ok(())
    })();
    let _ = stream.shutdown(Shutdown::Both);
    let _ = crossterm::execute!(
        io::stdout(),
        cursor::Show,
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    result
}

#[derive(Clone, Default)]
struct FrameWriter(Arc<Mutex<Vec<u8>>>);
impl Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Client {
    view: View,
    terminal: Terminal<CrosstermBackend<FrameWriter>>,
    rendered: FrameWriter,
    output: SyncSender<Response>,
    area: Rect,
    interacted: u64,
}
enum Incoming {
    Hello { stream: TcpStream, request: Request },
    Input(u64, Event),
    Closed(u64),
}
fn area(width: u16, height: u16) -> Rect {
    // Leave the last Windows console column untouched. ConPTY's optimized
    // output otherwise relies on pending-wrap behavior across erase commands,
    // which can corrupt a terminal emulator receiving a newly attached frame.
    let width = width
        .clamp(1, 240)
        .saturating_sub(u16::from(cfg!(windows)))
        .max(1);
    Rect::new(0, 0, width, height.clamp(1, 100))
}
fn renderer(
    area: Rect,
    mut writer: FrameWriter,
) -> io::Result<Terminal<CrosstermBackend<FrameWriter>>> {
    writer.write_all(b"\x1b[2J\x1b[H")?;
    Terminal::with_options(
        CrosstermBackend::new(writer),
        TerminalOptions {
            viewport: Viewport::Fixed(area),
        },
    )
}
fn attach_client(
    mut stream: TcpStream,
    id: u64,
    incoming: SyncSender<Incoming>,
    app: &App,
    area: Rect,
    interacted: u64,
) -> Result<Client> {
    let mut reader = stream.try_clone()?;
    let (output, responses): (SyncSender<Response>, Receiver<Response>) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        while let Ok(response) = responses.recv() {
            if packet(&mut stream, &response).is_err() {
                break;
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
    });
    std::thread::spawn(move || {
        while let Ok(Request::Input(event)) = receive(&mut reader) {
            if incoming.send(Incoming::Input(id, event)).is_err() {
                return;
            }
        }
        let _ = incoming.send(Incoming::Closed(id));
    });
    let rendered = FrameWriter::default();
    Ok(Client {
        view: app.view(),
        terminal: renderer(area, rendered.clone())?,
        rendered,
        output,
        area,
        interacted,
    })
}
fn input(app: &mut App, area: Rect, event: Event) -> Result<bool> {
    app.resized(area);
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => app.key(key),
        Event::Paste(text) => {
            if let Some(path) = &mut app.workspace {
                path.push_str(&text.replace(['\r', '\n'], ""));
            } else {
                app.panes[app.active].send(text.as_bytes())?;
            }
            Ok(false)
        }
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    return app.click(area, mouse.column, mouse.row);
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    app.context_menu(area, mouse.column, mouse.row)
                }
                MouseEventKind::Drag(MouseButton::Left) => app.drag(mouse.column, mouse.row),
                MouseEventKind::Up(MouseButton::Left) => app.end_drag(),
                _ => {}
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

pub fn serve(name: &str, executable: PathBuf, directories: Vec<PathBuf>) -> Result<()> {
    let result = serve_inner(name, executable, directories);
    if let Err(error) = &result
        && let Ok(directory) = root()
        && let Ok(mut log) = private_file(&directory.join(format!("{name}.log")), true)
    {
        let _ = writeln!(log, "{error}");
    }
    result
}
fn serve_inner(name: &str, executable: PathBuf, directories: Vec<PathBuf>) -> Result<()> {
    validate_name(name)?;
    let directory = root()?;
    let lock = private_file(&directory.join(format!("{name}.lock")), false)?;
    lock.try_lock()
        .map_err(|_| "Muxer session is already running")?;
    let mut app = App::new(executable);
    app.persistent = true;
    for cwd in directories {
        app.add_space(cwd)?;
    }
    app.select_space(0);
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let endpoint = Endpoint {
        protocol: PROTOCOL,
        port: listener.local_addr()?.port(),
        token: uuid::Uuid::new_v4().to_string(),
        pid: std::process::id(),
    };
    let endpoint_path = directory.join(format!("{name}.endpoint"));
    let temporary = directory.join(format!("{name}.endpoint.tmp"));
    let mut file = private_file(&temporary, false)?;
    file.set_len(0)?;
    file.write_all(&serde_json::to_vec(&endpoint)?)?;
    file.sync_all()?;
    drop(file);
    if endpoint_path.exists() {
        fs::remove_file(&endpoint_path)?;
    }
    fs::rename(&temporary, &endpoint_path)?;
    let result = run_server(&mut app, listener, &endpoint);
    // Dropping App kills and reaps PTYs before publishing that the server stopped.
    drop(app);
    let _ = fs::remove_file(endpoint_path);
    drop(lock);
    result
}
fn run_server(app: &mut App, listener: TcpListener, endpoint: &Endpoint) -> Result<()> {
    listener.set_nonblocking(true)?;
    let (incoming, events) = mpsc::sync_channel(256);
    let mut clients: std::collections::BTreeMap<u64, Client> = Default::default();
    let mut next_id = 1;
    let mut generation = 1;
    let mut last_view = app.view();
    let handshakes = Arc::new(AtomicUsize::new(0));
    loop {
        // Authentication reads happen outside the event loop so a half-written
        // local connection cannot freeze terminal rendering or other clients.
        while let Ok((mut stream, _)) = listener.accept() {
            if handshakes.fetch_add(1, Ordering::Relaxed) >= 32 {
                handshakes.fetch_sub(1, Ordering::Relaxed);
                continue;
            }
            // Windows accepted sockets inherit the listener's nonblocking mode.
            // Reader/writer threads use blocking I/O with bounded timeouts.
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            stream.set_nodelay(true)?;
            let sender = incoming.clone();
            let pending = handshakes.clone();
            std::thread::spawn(move || {
                if let Ok(request) = receive::<Request>(&mut stream) {
                    let _ = sender.send(Incoming::Hello { stream, request });
                }
                pending.fetch_sub(1, Ordering::Relaxed);
            });
        }
        for _ in 0..256 {
            let Ok(message) = events.try_recv() else {
                break;
            };
            match message {
                Incoming::Hello {
                    mut stream,
                    request:
                        Request::Hello {
                            protocol,
                            token,
                            operation,
                            width,
                            height,
                        },
                } => {
                    if protocol != PROTOCOL || token != endpoint.token {
                        let _ = packet(
                            &mut stream,
                            &Response::Error("Muxer authentication or protocol mismatch".into()),
                        );
                        continue;
                    }
                    let read = operation
                        .strip_prefix("read:")
                        .and_then(|id| id.parse::<u64>().ok());
                    if !matches!(operation.as_str(), "attach" | "status" | "stop") && read.is_none()
                    {
                        let _ = packet(
                            &mut stream,
                            &Response::Error("Unknown Muxer operation".into()),
                        );
                        continue;
                    }
                    if packet(&mut stream, &Response::Ready { pid: endpoint.pid }).is_err() {
                        continue;
                    }
                    if operation == "stop" {
                        return Ok(());
                    }
                    if let Some(id) = read {
                        let response = match app.panes.iter().find(|pane| pane.id == id) {
                            Some(pane) => {
                                Response::Text(pane.parser.lock().unwrap().screen().contents())
                            }
                            None => Response::Error("Pane does not exist".into()),
                        };
                        let _ = packet(&mut stream, &response);
                    }
                    if operation == "attach" {
                        if clients.len() >= 16 {
                            let _ = packet(
                                &mut stream,
                                &Response::Error("Sixteen clients are already attached".into()),
                            );
                            continue;
                        }
                        stream.set_read_timeout(None)?;
                        app.use_view(&last_view);
                        generation += 1;
                        let client = attach_client(
                            stream,
                            next_id,
                            incoming.clone(),
                            app,
                            area(width, height),
                            generation,
                        )?;
                        clients.insert(next_id, client);
                        next_id += 1;
                    }
                }
                Incoming::Input(id, event) => {
                    let Some(client) = clients.get_mut(&id) else {
                        continue;
                    };
                    app.use_view(&client.view);
                    if let Event::Resize(width, height) = event {
                        client.area = area(width, height);
                        client.terminal = renderer(client.area, client.rendered.clone())?;
                    }
                    generation += 1;
                    client.interacted = generation;
                    let detach = input(app, client.area, event)?;
                    if app.panes.is_empty() {
                        return Ok(());
                    }
                    client.view = app.view();
                    last_view = client.view.clone();
                    if detach {
                        let _ = client.output.try_send(Response::Detached);
                        clients.remove(&id);
                    }
                }
                Incoming::Closed(id) => {
                    clients.remove(&id);
                }
                _ => {}
            }
        }
        for pane in &mut app.panes {
            pane.poll()?;
        }
        // The last client to interact with a tab owns its PTY dimensions.
        // Other tabs follow their own viewers, rather than alternating sizes
        // every frame when differently sized terminals share one tab.
        let mut owners = std::collections::BTreeMap::<u64, (u64, u64)>::new();
        for (id, client) in &clients {
            app.use_view(&client.view);
            let owner = owners.entry(app.selected_tab()).or_default();
            if client.interacted >= owner.1 {
                *owner = (*id, client.interacted);
            }
        }
        for (id, _) in owners.values() {
            let client = &clients[id];
            app.use_view(&client.view);
            for (index, rect) in app.visible(client.area) {
                app.panes[index].resize(pane_inner(rect))?;
            }
        }
        let mut closed = Vec::new();
        for (id, client) in &mut clients {
            app.use_view(&client.view);
            app.resized(client.area);
            client.view = app.view();
            client.terminal.draw(|frame| app.draw(frame))?;
            let bytes = std::mem::take(&mut *client.rendered.0.lock().unwrap());
            if !bytes.is_empty() && client.output.try_send(Response::Frame(bytes)).is_err() {
                closed.push(*id);
            }
        }
        for id in closed {
            clients.remove(&id);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
