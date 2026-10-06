use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        io::AsRawFd,
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const CHILD_ARGUMENT: &str = "--boxer-session-child";
const DAEMON_ARGUMENT: &str = "--boxer-session-daemon";
const HISTORY_LIMIT: usize = 64 * 1024;
static SESSION_CHILD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_child(active: bool) {
    SESSION_CHILD.store(active, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_child() -> bool {
    SESSION_CHILD.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    version: u32,
    id: String,
    pid: u32,
    created_unix_ms: u128,
    workspace: String,
    command: String,
    status: String,
    exit_code: Option<i32>,
}

pub fn command(arguments: &[std::ffi::OsString]) -> io::Result<i32> {
    let (action, offset) = if arguments.first().is_some_and(|arg| arg == "sessions") {
        (arguments.get(1).and_then(|arg| arg.to_str()), 1)
    } else {
        (arguments.first().and_then(|arg| arg.to_str()), 0)
    };
    match action {
        Some("list" | "ps") if arguments.len() == 1 + offset => list(),
        Some("inspect") if arguments.len() == 2 + offset => inspect(text(arguments, 1 + offset)?),
        Some("stop") if arguments.len() == 2 + offset => stop(text(arguments, 1 + offset)?),
        Some("logs") if arguments.len() == 2 + offset => logs(text(arguments, 1 + offset)?),
        Some("prune") if arguments.len() == 1 + offset => prune(),
        Some("attach") if arguments.len() == 2 + offset => attach(text(arguments, 1 + offset)?),
        Some("detach") if arguments.len() == 2 + offset => {
            control(text(arguments, 1 + offset)?, b'D')
        }
        _ => Err(io::Error::other(
            "Usage: boxer sessions list | attach <id> | detach <id> | inspect <id> | logs <id> | stop <id> | prune",
        )),
    }
}

pub fn start(
    arguments: &[std::ffi::OsString],
    workspace: &std::path::Path,
    command: &str,
) -> io::Result<i32> {
    let root = root()?;
    let id = Uuid::new_v4().hyphenated().to_string();
    let stdout_path = root.join(format!("{id}.out"));
    let stderr_path = root.join(format!("{id}.err"));
    let stdout = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stdout_path)?;
    let stderr = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stderr_path)?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .arg(DAEMON_ARGUMENT)
        .arg(&id)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    // The session broker and its terminal remain alive after this launcher exits.
    unsafe {
        child.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let process = child.spawn()?;
    let session = Session {
        version: 1,
        id: id.clone(),
        pid: process.id(),
        created_unix_ms: now_ms()?,
        workspace: workspace.to_string_lossy().into_owned(),
        command: command.to_owned(),
        status: "running".into(),
        exit_code: None,
    };
    if let Err(error) = save(&root, &session) {
        // SAFETY: the new child owns this process group.
        unsafe {
            libc::kill(-(process.id() as i32), libc::SIGKILL);
        }
        let _ = process.wait_with_output();
        return Err(error);
    }
    println!("Detached Boxer session: {id}");
    println!("Inspect: boxer sessions inspect {id}");
    println!("Attach: boxer sessions attach {id}");
    println!("Logs: boxer sessions logs {id}");
    let pid = process.id();
    drop(process);
    if let Err(error) = wait_for_socket(&root, &id) {
        // SAFETY: this is the session process group created immediately above.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        return Err(error);
    }
    Ok(0)
}

pub fn daemon_id(arguments: &[std::ffi::OsString]) -> Option<String> {
    (arguments.first().is_some_and(|arg| arg == DAEMON_ARGUMENT))
        .then(|| {
            arguments
                .get(1)
                .map(|arg| arg.to_string_lossy().into_owned())
        })
        .flatten()
}

pub fn daemon(arguments: &[std::ffi::OsString]) -> io::Result<i32> {
    let id = text(arguments, 1)?.to_owned();
    let root = root()?;
    read(&root, &id)?;
    let socket_path = socket_path(&root, &id);
    let control_path = control_path(&root, &id);
    let _ = fs::remove_file(&socket_path);
    let _ = fs::remove_file(&control_path);
    let listener = UnixListener::bind(&socket_path)?;
    let control_listener = UnixListener::bind(&control_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    fs::set_permissions(&control_path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    control_listener.set_nonblocking(true)?;

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(io::Error::other)?;
    let mut command = CommandBuilder::new(std::env::current_exe()?);
    command.arg(CHILD_ARGUMENT);
    command.arg(&id);
    command.args(&arguments[2..]);
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(io::Error::other)?;
    drop(pair.slave);
    let mut writer = pair.master.take_writer().map_err(io::Error::other)?;
    let reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
    let (output_tx, output_rx) = mpsc::sync_channel::<Vec<u8>>(64);
    thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if output_tx.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut active: Option<UnixStream> = None;
    let mut pending_output = VecDeque::new();
    let mut history = VecDeque::with_capacity(HISTORY_LIMIT);
    let mut log = OpenOptions::new()
        .append(true)
        .open(root.join(format!("{id}.out")))?;
    let mut stop_requested = false;
    loop {
        while let Ok(bytes) = output_rx.try_recv() {
            log.write_all(&bytes)?;
            for byte in &bytes {
                if history.len() == HISTORY_LIMIT {
                    history.pop_front();
                }
                history.push_back(*byte);
            }
            if active.is_some() {
                pending_output.extend(bytes);
            }
        }

        while let Ok((mut control, _)) = control_listener.accept() {
            control.set_read_timeout(Some(Duration::from_millis(200)))?;
            let mut message = [0; 5];
            if control.read_exact(&mut message).is_ok() {
                match message[0] {
                    b'R' => {
                        let rows = u16::from_be_bytes([message[1], message[2]]).max(1);
                        let cols = u16::from_be_bytes([message[3], message[4]]).max(1);
                        // PTY resize support varies by platform. A rejected size
                        // update must not terminate an otherwise healthy session.
                        let _ = pair.master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    b'D' => {
                        active = None;
                        pending_output.clear();
                    }
                    b'K' => {
                        stop_requested = true;
                        if let Some(pid) = child.process_id() {
                            signal_child_group(pid, libc::SIGINT);
                        }
                        let _ = writer.write_all(&[3]);
                    }
                    _ => {}
                }
            }
        }

        while let Ok((mut stream, _)) = listener.accept() {
            if active.is_some() {
                let _ =
                    stream.write_all(b"\r\n[Boxer session already has an attached terminal]\r\n");
                continue;
            }
            stream.set_nonblocking(true)?;
            pending_output.extend(history.iter().copied());
            active = Some(stream);
        }

        if stop_requested {
            for _ in 0..50 {
                if child.try_wait()?.is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            if child.try_wait()?.is_none() {
                if let Some(pid) = child.process_id() {
                    signal_child_group(pid, libc::SIGKILL);
                }
                let _ = child.kill();
            }
        }
        if let Some(status) = child.try_wait()? {
            if let Some(pid) = child.process_id() {
                signal_child_group(pid, libc::SIGTERM);
            }
            let mut finished = read(&root, &id)?;
            finished.status = if stop_requested {
                "stopped"
            } else {
                "finished"
            }
            .into();
            finished.exit_code = Some(status.exit_code() as i32);
            save(&root, &finished)?;
            break;
        }
        let mut disconnect = false;
        if let Some(stream) = active.as_mut() {
            flush_output(stream, &mut pending_output);
            if pending_output.len() > HISTORY_LIMIT * 4 {
                disconnect = true;
            }
            let mut input = [0; 8192];
            if !disconnect {
                match stream.read(&mut input) {
                    Ok(0) => disconnect = true,
                    Ok(count) => writer.write_all(&input[..count])?,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => disconnect = true,
                }
            }
        }
        if disconnect {
            active = None;
            pending_output.clear();
        }
        if stop_requested && child.try_wait()?.is_some() {
            continue;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = fs::remove_file(socket_path);
    let _ = fs::remove_file(control_path);
    Ok(0)
}

pub fn child_id(arguments: &[std::ffi::OsString]) -> Option<String> {
    (arguments.first().is_some_and(|arg| arg == CHILD_ARGUMENT))
        .then(|| {
            arguments
                .get(1)
                .map(|arg| arg.to_string_lossy().into_owned())
        })
        .flatten()
}

pub fn await_registered(id: &str) -> io::Result<()> {
    let root = root()?;
    let path = root.join(format!(
        "{}.json",
        Uuid::parse_str(id).map_err(io::Error::other)?.hyphenated()
    ));
    for _ in 0..200 {
        if path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(io::Error::other(
        "Detached session was not registered by its launcher",
    ))
}

pub fn finish(id: &str, code: i32) -> io::Result<()> {
    let root = root()?;
    let mut session = read(&root, id)?;
    session.status = "finished".into();
    session.exit_code = Some(code);
    save(&root, &session)
}

fn list() -> io::Result<i32> {
    let root = root()?;
    println!("ID\tSTATUS\tPID\tCOMMAND");
    for mut session in all(&root)? {
        refresh_status(&mut session);
        println!(
            "{}\t{}\t{}\t{}",
            session.id, session.status, session.pid, session.command
        );
        if session.status != "running" {
            save(&root, &session)?;
        }
    }
    Ok(0)
}

fn inspect(id: &str) -> io::Result<i32> {
    let root = root()?;
    let mut session = read(&root, id)?;
    refresh_status(&mut session);
    save(&root, &session)?;
    println!(
        "ID: {}\nStatus: {}\nPID: {}\nCommand: {}\nWorkspace: {}\nStarted: {}\nExit code: {}",
        session.id,
        session.status,
        session.pid,
        session.command,
        session.workspace,
        session.created_unix_ms,
        session
            .exit_code
            .map_or_else(|| "unknown".into(), |code| code.to_string())
    );
    println!(
        "Stdout: {}\nStderr: {}",
        root.join(format!("{id}.out")).display(),
        root.join(format!("{id}.err")).display()
    );
    Ok(0)
}

fn stop(id: &str) -> io::Result<i32> {
    let root = root()?;
    let mut session = read(&root, id)?;
    refresh_status(&mut session);
    if session.status == "running" {
        if !is_live_session(session.pid) {
            session.status = "finished".into();
            save(&root, &session)?;
            println!("Session {}: {}", id, session.status);
            return Ok(0);
        }
        control(id, b'K')?;
        for _ in 0..60 {
            thread::sleep(Duration::from_millis(100));
            session = read(&root, id)?;
            if session.status != "running" {
                break;
            }
        }
        if session.status == "running" {
            // SAFETY: the process group was created by this detached Boxer session.
            unsafe {
                libc::kill(-(session.pid as i32), libc::SIGKILL);
            }
            session.status = "stopped".into();
            save(&root, &session)?;
        }
    }
    println!("Session {}: {}", id, session.status);
    Ok(0)
}

fn logs(id: &str) -> io::Result<i32> {
    let root = root()?;
    read(&root, id)?;
    for suffix in ["out", "err"] {
        println!("--- {suffix} ---");
        let path = root.join(format!("{id}.{suffix}"));
        if path.exists() {
            let mut file = File::open(path)?;
            io::copy(&mut file, &mut io::stdout())?;
        }
    }
    Ok(0)
}

fn prune() -> io::Result<i32> {
    let root = root()?;
    let mut removed = 0;
    for mut session in all(&root)? {
        refresh_status(&mut session);
        if session.status != "running" {
            let id = session.id.clone();
            fs::remove_file(root.join(format!("{id}.json")))?;
            let _ = fs::remove_file(root.join(format!("{id}.out")));
            let _ = fs::remove_file(root.join(format!("{id}.err")));
            let _ = fs::remove_file(socket_path(&root, &id));
            let _ = fs::remove_file(control_path(&root, &id));
            removed += 1;
        }
    }
    println!("Removed {removed} finished session(s)");
    Ok(0)
}

fn refresh_status(session: &mut Session) {
    if session.status == "running" && !is_live_session(session.pid) {
        session.status = "finished".into();
    }
}

fn is_live_session(pid: u32) -> bool {
    // Boxer starts each detached process as a new session leader. Checking that
    // identity as well as liveness avoids signaling an unrelated process after
    // a stale PID has been reused.
    // SAFETY: getsid and signal 0 only inspect this user's recorded process.
    unsafe { libc::getsid(pid as i32) == pid as i32 && libc::kill(pid as i32, 0) == 0 }
}

fn all(root: &std::path::Path) -> io::Result<Vec<Session>> {
    let mut sessions = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let data = fs::read(entry.path())?;
            sessions.push(serde_json::from_slice(&data).map_err(io::Error::other)?);
        }
    }
    sessions.sort_by_key(|session: &Session| session.created_unix_ms);
    Ok(sessions)
}

fn read(root: &std::path::Path, id: &str) -> io::Result<Session> {
    let id = Uuid::parse_str(id).map_err(|_| io::Error::other("Invalid session ID"))?;
    serde_json::from_slice(&fs::read(root.join(format!("{}.json", id.hyphenated())))?)
        .map_err(io::Error::other)
}

fn save(root: &std::path::Path, session: &Session) -> io::Result<()> {
    let path = root.join(format!("{}.json", session.id));
    let temporary = root.join(format!("{}.tmp", session.id));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(session).map_err(io::Error::other)?,
    )?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn root() -> io::Result<PathBuf> {
    let path = std::env::var_os("BOXER_SESSIONS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".boxer").join("sessions"))
        })
        .ok_or_else(|| io::Error::other("Cannot determine the Boxer session directory"))?;
    fs::create_dir_all(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    path.canonicalize()
}

fn now_ms() -> io::Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis())
}

fn text(arguments: &[std::ffi::OsString], index: usize) -> io::Result<&str> {
    arguments
        .get(index)
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::other("Missing session ID"))
}

fn wait_for_socket(root: &std::path::Path, id: &str) -> io::Result<()> {
    let path = socket_path(root, id);
    for _ in 0..200 {
        if path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let error_log = root.join(format!("{id}.err"));
    let details = fs::read_to_string(&error_log).unwrap_or_default();
    Err(io::Error::other(format!(
        "Detached session did not start; daemon error: {} (log: {})",
        if details.trim().is_empty() {
            "no details were recorded"
        } else {
            details.trim()
        },
        error_log.display()
    )))
}

fn socket_path(_root: &std::path::Path, id: &str) -> PathBuf {
    runtime_socket_path(id, "s")
}

fn control_path(_root: &std::path::Path, id: &str) -> PathBuf {
    runtime_socket_path(id, "c")
}

fn runtime_socket_path(id: &str, suffix: &str) -> PathBuf {
    let compact_id = Uuid::parse_str(id)
        .map(|id| id.simple().to_string())
        .unwrap_or_else(|_| "invalid".into());
    let uid = unsafe { libc::geteuid() };
    std::env::temp_dir().join(format!(
        "b{uid}-{}{suffix}",
        &compact_id[..20.min(compact_id.len())]
    ))
}

fn control(id: &str, action: u8) -> io::Result<i32> {
    let id = Uuid::parse_str(id).map_err(|_| io::Error::other("Invalid session ID"))?;
    let root = root()?;
    let mut stream = UnixStream::connect(control_path(&root, &id.hyphenated().to_string()))?;
    stream.write_all(&[action, 0, 0, 0, 0])?;
    Ok(0)
}

fn flush_output(stream: &mut UnixStream, output: &mut VecDeque<u8>) {
    if output.is_empty() {
        return;
    }
    let bytes = output.make_contiguous();
    match stream.write(bytes) {
        Ok(count) => {
            output.drain(..count);
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
        Err(_) => output.clear(),
    }
}

fn signal_child_group(pid: u32, signal: i32) {
    // The PTY child should lead its own process group. Verify before signaling
    // so a stale or non-leader PID can never target an unrelated group.
    // SAFETY: getpgid and kill only inspect/signal the child PID recorded by the PTY.
    unsafe {
        if libc::getpgid(pid as i32) == pid as i32 {
            libc::kill(-(pid as i32), signal);
        }
    }
}

fn attach(id: &str) -> io::Result<i32> {
    let parsed = Uuid::parse_str(id).map_err(|_| io::Error::other("Invalid session ID"))?;
    let root = root()?;
    let session = read(&root, &parsed.hyphenated().to_string())?;
    if session.status != "running" {
        return Err(io::Error::other("Session is not running"));
    }
    let _terminal = RawTerminal::enter()?;
    let (mut rows, mut cols) = terminal_size()?;
    send_resize(&root, &session.id, rows, cols)?;
    let path = socket_path(&root, &session.id);
    let mut stream = None;
    for _ in 0..200 {
        match UnixStream::connect(&path) {
            Ok(connection) => {
                stream = Some(connection);
                break;
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.kind() == io::ErrorKind::ConnectionRefused =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    let mut stream =
        stream.ok_or_else(|| io::Error::other("Could not connect to the Boxer session"))?;
    send_resize(&root, &session.id, rows, cols)?;
    let mut pending_escape = false;
    let mut buffer = [0u8; 8192];
    loop {
        let mut descriptors = [
            libc::pollfd {
                fd: io::stdin().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: descriptors points to two initialized pollfd values for this call.
        let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as _, 100) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        if descriptors[0].revents & libc::POLLIN != 0 {
            // SAFETY: buffer is writable and stdin is an open descriptor.
            let count =
                unsafe { libc::read(descriptors[0].fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count > 0 {
                let mut forwarded = Vec::with_capacity(count as usize);
                let mut detach = false;
                for byte in &buffer[..count as usize] {
                    if pending_escape {
                        pending_escape = false;
                        if *byte == b'd' {
                            detach = true;
                            break;
                        }
                        forwarded.push(0x1d);
                    }
                    if *byte == 0x1d {
                        pending_escape = true;
                    } else {
                        forwarded.push(*byte);
                    }
                }
                if detach {
                    if pending_escape {
                        forwarded.push(0x1d);
                    }
                    break;
                }
                if !forwarded.is_empty() {
                    stream.write_all(&forwarded)?;
                }
            }
        }
        if descriptors[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => io::stdout().write_all(&buffer[..count])?,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        let (next_rows, next_cols) = terminal_size()?;
        if (next_rows, next_cols) != (rows, cols) {
            rows = next_rows;
            cols = next_cols;
            send_resize(&root, &session.id, rows, cols)?;
        }
    }
    if pending_escape {
        stream.write_all(&[0x1d])?;
    }
    Ok(0)
}

struct RawTerminal(libc::termios);

impl RawTerminal {
    fn enter() -> io::Result<Self> {
        if unsafe { libc::isatty(io::stdin().as_raw_fd()) } != 1 {
            return Err(io::Error::other(
                "Attaching requires an interactive terminal",
            ));
        }
        // SAFETY: termios is initialized by tcgetattr before it is read.
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(io::stdin().as_raw_fd(), &mut original) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = original;
        unsafe {
            libc::cfmakeraw(&mut raw);
        }
        if unsafe { libc::tcsetattr(io::stdin().as_raw_fd(), libc::TCSAFLUSH, &raw) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(original))
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        // SAFETY: this restores the terminal settings saved by `enter`.
        unsafe {
            libc::tcsetattr(io::stdin().as_raw_fd(), libc::TCSAFLUSH, &self.0);
        }
    }
}

fn terminal_size() -> io::Result<(u16, u16)> {
    // SAFETY: winsize is written by ioctl when stdin is a terminal.
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(io::stdin().as_raw_fd(), libc::TIOCGWINSZ, &mut size) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((size.ws_row.max(1), size.ws_col.max(1)))
}

fn send_resize(root: &std::path::Path, id: &str, rows: u16, cols: u16) -> io::Result<()> {
    let mut stream = UnixStream::connect(control_path(root, id))?;
    stream.write_all(&[
        b'R',
        (rows >> 8) as u8,
        rows as u8,
        (cols >> 8) as u8,
        cols as u8,
    ])
}
