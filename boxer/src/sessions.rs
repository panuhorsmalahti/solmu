use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const CHILD_ARGUMENT: &str = "--boxer-session-child";
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
    match arguments.get(1).and_then(|arg| arg.to_str()) {
        Some("list") if arguments.len() == 2 => list(),
        Some("inspect") if arguments.len() == 3 => inspect(text(arguments, 2)?),
        Some("stop") if arguments.len() == 3 => stop(text(arguments, 2)?),
        Some("logs") if arguments.len() == 3 => logs(text(arguments, 2)?),
        Some("prune") if arguments.len() == 2 => prune(),
        _ => Err(io::Error::other(
            "Usage: boxer sessions list | inspect <id> | logs <id> | stop <id> | prune",
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
        .arg(CHILD_ARGUMENT)
        .arg(&id)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    // A private session survives terminal closure, and its process group lets
    // `stop` terminate the sandbox supervisor and every ordinary descendant.
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
    println!("Logs: boxer sessions logs {id}");
    drop(process);
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
        // SAFETY: process group is created by Boxer with the recorded leader PID.
        if unsafe { libc::kill(-(session.pid as i32), libc::SIGTERM) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::NotFound {
                return Err(error);
            }
        }
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(25));
            // SAFETY: signal 0 checks whether the session process group remains.
            if unsafe { libc::kill(-(session.pid as i32), 0) } < 0 {
                break;
            }
        }
        // SAFETY: the process group belongs to this detached Boxer session.
        unsafe {
            libc::kill(-(session.pid as i32), libc::SIGKILL);
        }
        session.status = "stopped".into();
        save(&root, &session)?;
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
    Ok(path.canonicalize()?)
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
