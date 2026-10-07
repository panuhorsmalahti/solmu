use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf, time::SystemTime};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    },
};

#[derive(Serialize, Deserialize)]
struct Launch {
    version: u32,
    id: String,
    pid: u32,
    created_unix_ms: u128,
    workspace: String,
    command: String,
    status: String,
    detached: bool,
    attached: bool,
    exit_code: Option<i32>,
}

pub fn register(workspace: &std::path::Path, command: &str) -> io::Result<String> {
    let id = uuid::Uuid::new_v4().hyphenated().to_string();
    save(&Launch {
        version: 1,
        id: id.clone(),
        pid: std::process::id(),
        created_unix_ms: now_ms()?,
        workspace: workspace.to_string_lossy().into_owned(),
        command: command.to_owned(),
        status: "running".into(),
        detached: false,
        attached: true,
        exit_code: None,
    })?;
    Ok(id)
}

pub fn finish(id: &str, exit_code: i32) -> io::Result<()> {
    let mut launch = read(id)?;
    launch.status = "finished".into();
    launch.exit_code = Some(exit_code);
    save(&launch)
}

pub fn set_child_pid(id: &str, pid: u32) -> io::Result<()> {
    let mut launch = read(id)?;
    launch.pid = pid;
    save(&launch)
}

pub fn command(arguments: &[std::ffi::OsString]) -> io::Result<i32> {
    let (action, offset) = if arguments.first().is_some_and(|arg| arg == "sessions") {
        (arguments.get(1).and_then(|arg| arg.to_str()), 1)
    } else {
        (arguments.first().and_then(|arg| arg.to_str()), 0)
    };
    if action == Some("stop")
        && arguments
            .get(1 + offset)
            .and_then(|arg| arg.to_str())
            .is_some()
    {
        let id = arguments[1 + offset].to_string_lossy();
        let force = arguments
            .get(2 + offset)
            .is_some_and(|arg| arg == "--force");
        if arguments.len() != 2 + offset + usize::from(force) {
            return Err(io::Error::other("Usage: boxer stop <id> [--force]"));
        }
        return stop(&id, force);
    }
    if !matches!(action, Some("ps" | "list")) {
        return Err(io::Error::other(
            "Usage: boxer ps|sessions list [--all] [--json]",
        ));
    }
    let mut show_all = false;
    let mut json = false;
    for argument in &arguments[1 + offset..] {
        match argument.to_str() {
            Some("--all") => show_all = true,
            Some("--json") => json = true,
            _ => {
                return Err(io::Error::other(
                    "Usage: boxer ps|sessions list [--all] [--json]",
                ));
            }
        }
    }

    let root = root()?;
    let mut launches = Vec::new();
    for entry in fs::read_dir(&root)? {
        let path = entry?.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".launch.json"))
        {
            continue;
        }
        let mut launch: Launch =
            serde_json::from_slice(&fs::read(&path)?).map_err(io::Error::other)?;
        if launch.status == "running" && !process_is_running(launch.pid) {
            launch.status = "finished".into();
            save_at(&path, &launch)?;
        }
        if show_all || launch.status == "running" {
            launches.push(launch);
        }
    }
    launches.sort_by_key(|launch| launch.created_unix_ms);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&launches).map_err(io::Error::other)?
        );
    } else {
        println!("ID\tSTATUS\tATTACHMENT\tPID\tCOMMAND");
        for launch in launches {
            println!(
                "{}\t{}\tattached\t{}\t{}",
                launch.id, launch.status, launch.pid, launch.command
            );
        }
    }
    Ok(0)
}

fn stop(id: &str, force: bool) -> io::Result<i32> {
    let mut launch = read(id)?;
    if launch.status == "running" {
        let process =
            unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, launch.pid) };
        if process.is_null() {
            launch.status = "finished".into();
            save(&launch)?;
        } else {
            let code = if force { 1 } else { 0 };
            let terminated = unsafe { TerminateProcess(process, code) } != 0;
            let error = (!terminated).then(io::Error::last_os_error);
            unsafe { CloseHandle(process) };
            if let Some(error) = error {
                return Err(error);
            }
            launch.status = "finished".into();
            launch.exit_code = Some(code as i32);
            save(&launch)?;
        }
    }
    println!("Session {}: {}", launch.id, launch.status);
    Ok(0)
}

fn process_is_running(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return false;
        }
        let running = WaitForSingleObject(process, 0) == 0x00000102;
        CloseHandle(process);
        running
    }
}

fn root() -> io::Result<PathBuf> {
    let path = std::env::var_os("BOXER_SESSIONS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".boxer").join("sessions"))
        })
        .ok_or_else(|| io::Error::other("Cannot determine the Boxer session directory"))?;
    fs::create_dir_all(&path)?;
    path.canonicalize()
}

fn read(id: &str) -> io::Result<Launch> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| io::Error::other("Invalid launch ID"))?;
    serde_json::from_slice(&fs::read(
        root()?.join(format!("{}.launch.json", id.hyphenated())),
    )?)
    .map_err(io::Error::other)
}

fn save(launch: &Launch) -> io::Result<()> {
    save_at(&root()?.join(format!("{}.launch.json", launch.id)), launch)
}

fn save_at(path: &std::path::Path, launch: &Launch) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(launch).map_err(io::Error::other)?,
    )?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            fs::remove_file(path)?;
            fs::rename(temporary, path).map_err(|_| error)
        }
        Err(error) => Err(error),
    }
}

fn now_ms() -> io::Result<u128> {
    Ok(SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis())
}
