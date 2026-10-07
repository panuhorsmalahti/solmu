use crate::network::{HostPattern, Target};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Once,
    Session,
    Deny,
}

pub enum Resolution {
    Once,
    Session(HostPattern),
    Deny,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    target: Target,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Response {
    decision: Decision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host_pattern: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditPayload {
    request_id: String,
    target: Target,
    decision: Decision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host_pattern: Option<String>,
    reason: String,
    timestamp_ms: u128,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    sequence: u64,
    previous_mac: String,
    payload: AuditPayload,
    mac: String,
}

pub struct Supervisor {
    id: String,
    directory: PathBuf,
    running: Arc<AtomicBool>,
}

impl Supervisor {
    pub fn new() -> io::Result<Self> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let directory = directory(&id)?;
        fs::create_dir(&directory)?;
        private_directory(&directory)?;
        fs::create_dir(directory.join("requests"))?;
        fs::create_dir(directory.join("responses"))?;
        eprintln!(
            "Boxer supervisor session: {id} (list requests with `boxer supervisor {id} list`)"
        );
        Ok(Self {
            id,
            directory,
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    pub fn request(&self, target: &Target) -> Resolution {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let request = Request {
            id: id.clone(),
            target: target.clone(),
        };
        let request_path = self.directory.join("requests").join(format!("{id}.json"));
        let response_path = self.directory.join("responses").join(format!("{id}.json"));
        if write_new_json(&request_path, &request).is_err() {
            return Resolution::Deny;
        }
        eprintln!(
            "Boxer approval needed for {}:{}; run `boxer supervisor {} list` in another terminal",
            target.host, target.port, self.id
        );
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let (mut decision, mut host_pattern, mut reason) = loop {
            if !self.running.load(Ordering::Acquire) {
                break (Decision::Deny, None, "session_stopped");
            }
            if Instant::now() >= deadline {
                break (Decision::Deny, None, "timed_out");
            }
            match fs::read(&response_path) {
                Ok(contents) => match serde_json::from_slice::<Response>(&contents) {
                    Ok(response) => {
                        let reason = match (response.decision, response.host_pattern.is_some()) {
                            (Decision::Once, _) => "approved_once",
                            (Decision::Session, true) => "approved_for_session_pattern",
                            (Decision::Session, false) => "approved_for_session",
                            (Decision::Deny, _) => "denied",
                        };
                        break (response.decision, response.host_pattern, reason);
                    }
                    Err(_) => thread::sleep(Duration::from_millis(25)),
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break (Decision::Deny, None, "request_channel_error"),
            }
        };
        let session_pattern = if matches!(decision, Decision::Session) {
            let requested = host_pattern
                .clone()
                .unwrap_or_else(|| format!("{}:{}", target.host, target.port));
            match HostPattern::parse(&requested) {
                Ok(pattern) if pattern.matches(&target.host, target.port) => Some(pattern),
                _ => {
                    decision = Decision::Deny;
                    host_pattern = None;
                    reason = "invalid_grant_pattern";
                    None
                }
            }
        } else {
            None
        };
        let timestamp_ms = match now_ms() {
            Ok(timestamp) => timestamp,
            Err(error) => {
                eprintln!("Boxer could not timestamp the network approval decision: {error}");
                let _ = fs::remove_file(request_path);
                let _ = fs::remove_file(response_path);
                return Resolution::Deny;
            }
        };
        if let Err(error) = append_audit(
            &self.directory,
            AuditPayload {
                request_id: id.clone(),
                target: target.clone(),
                decision,
                host_pattern,
                reason: reason.to_owned(),
                timestamp_ms,
            },
        ) {
            eprintln!("Boxer could not record the network approval decision: {error}");
            let _ = fs::remove_file(request_path);
            let _ = fs::remove_file(response_path);
            return Resolution::Deny;
        }
        let _ = fs::remove_file(request_path);
        let _ = fs::remove_file(response_path);
        match (decision, session_pattern) {
            (Decision::Once, _) => Resolution::Once,
            (Decision::Session, Some(pattern)) => Resolution::Session(pattern),
            _ => Resolution::Deny,
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
        for name in ["requests", "responses"] {
            let _ = fs::remove_dir_all(self.directory.join(name));
        }
    }
}

pub fn command(arguments: &[OsString]) -> io::Result<i32> {
    if arguments.len() < 3 {
        return Err(usage());
    }
    let session = text(&arguments[1], "session ID")?;
    let directory = directory(session)?;
    let action = text(&arguments[2], "action")?;
    match action {
        "list" if arguments.len() == 3 || arguments.len() == 4 && arguments[3] == "--json" => {
            let requests = list(&directory)?;
            if arguments
                .get(3)
                .is_some_and(|argument| argument == "--json")
            {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({"items": requests}))?
                );
            } else if requests.is_empty() {
                println!("No pending Boxer approvals");
            } else {
                for request in requests {
                    println!(
                        "{}\t{}:{}",
                        request.id, request.target.host, request.target.port
                    );
                }
            }
            Ok(0)
        }
        "history" if arguments.len() == 3 || arguments.len() == 4 && arguments[3] == "--json" => {
            let records = read_audit(&directory)?;
            verify_audit(&directory, &records)?;
            if arguments
                .get(3)
                .is_some_and(|argument| argument == "--json")
            {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({"items": records}))?
                );
            } else if records.is_empty() {
                println!("No network approval decisions recorded");
            } else {
                for record in records {
                    let pattern = record
                        .payload
                        .host_pattern
                        .as_deref()
                        .map(|pattern| format!("session pattern {pattern}"))
                        .unwrap_or_default();
                    println!(
                        "{}\t{}:{}\t{}\t{}",
                        record.payload.request_id,
                        record.payload.target.host,
                        record.payload.target.port,
                        record.payload.reason,
                        pattern
                    );
                }
            }
            Ok(0)
        }
        "approve" if (4..=7).contains(&arguments.len()) => {
            let request_id = text(&arguments[3], "request ID")?;
            validate_id(request_id)?;
            let decision = match arguments.get(4).and_then(|argument| argument.to_str()) {
                Some("--once") if arguments.len() == 5 => Decision::Once,
                Some("--session") if arguments.len() == 5 || arguments.len() == 7 => {
                    Decision::Session
                }
                _ => return Err(usage()),
            };
            let host_pattern = if arguments.len() == 7 {
                if arguments[5] != "--host" {
                    return Err(usage());
                }
                Some(text(&arguments[6], "host pattern")?)
            } else {
                None
            };
            if host_pattern.is_some() && !matches!(decision, Decision::Session) {
                return Err(usage());
            }
            respond(&directory, request_id, decision, host_pattern)?;
            println!(
                "Approved request {request_id} ({})",
                decision_name(decision)
            );
            if let Some(pattern) = host_pattern {
                println!("Session grant: {pattern}");
            }
            Ok(0)
        }
        "deny" if arguments.len() == 4 => {
            let request_id = text(&arguments[3], "request ID")?;
            validate_id(request_id)?;
            respond(&directory, request_id, Decision::Deny, None)?;
            println!("Denied request {request_id}");
            Ok(0)
        }
        _ => Err(usage()),
    }
}

fn list(directory: &std::path::Path) -> io::Result<Vec<Request>> {
    let requests_directory = directory.join("requests");
    let mut requests = Vec::new();
    for entry in fs::read_dir(requests_directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let request: Request =
                serde_json::from_slice(&fs::read(entry.path())?).map_err(io::Error::other)?;
            requests.push(request);
        }
    }
    requests.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(requests)
}

fn respond(
    directory: &std::path::Path,
    id: &str,
    decision: Decision,
    host_pattern: Option<&str>,
) -> io::Result<()> {
    let request = directory.join("requests").join(format!("{id}.json"));
    if !request.is_file() {
        return Err(io::Error::other("No such pending Boxer approval"));
    }
    let request: Request = serde_json::from_slice(&fs::read(request)?).map_err(io::Error::other)?;
    let host_pattern = match (decision, host_pattern) {
        (Decision::Session, Some(pattern)) => {
            let pattern = HostPattern::parse(pattern)?;
            if !pattern.matches(&request.target.host, request.target.port) {
                return Err(io::Error::other(
                    "Session host pattern must include the pending host and port",
                ));
            }
            Some(pattern.authority())
        }
        (Decision::Session, None) | (Decision::Once | Decision::Deny, None) => None,
        _ => return Err(usage()),
    };
    write_new_json(
        &directory.join("responses").join(format!("{id}.json")),
        &Response {
            decision,
            host_pattern,
        },
    )
}

fn write_new_json(path: &std::path::Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn root() -> io::Result<PathBuf> {
    let configured_root = std::env::var_os("BOXER_SUPERVISOR_DIR");
    let root = if let Some(path) = &configured_root {
        PathBuf::from(path)
    } else {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .ok_or_else(|| {
                io::Error::other("Cannot find the home directory for supervisor data")
            })?;
        home.join(".boxer").join("supervisor")
    };
    let existed = root.exists();
    fs::create_dir_all(&root)?;
    if fs::symlink_metadata(&root)?.file_type().is_symlink() {
        return Err(io::Error::other(
            "Boxer supervisor storage cannot be a symbolic link",
        ));
    }
    #[cfg(unix)]
    if configured_root.is_some() && existed {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&root)?.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "BOXER_SUPERVISOR_DIR must be private (permissions 0700 or stricter)",
            ));
        }
    }
    private_directory(&root)?;
    root.canonicalize()
}

fn directory(id: &str) -> io::Result<PathBuf> {
    validate_id(id)?;
    Ok(root()?.join(id))
}

fn private_directory(path: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn key(directory: &std::path::Path) -> io::Result<Vec<u8>> {
    let root = directory
        .parent()
        .ok_or_else(|| io::Error::other("Invalid supervisor directory"))?;
    let path = root.join("audit.key");
    if !path.exists() {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return fs::read(path),
            Err(error) => return Err(error),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        return Ok(bytes);
    }
    let bytes = fs::read(path)?;
    if bytes.len() != 32 {
        return Err(io::Error::other("Invalid Boxer supervisor audit key"));
    }
    Ok(bytes)
}

fn now_ms() -> io::Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis())
}

fn read_audit(directory: &std::path::Path) -> io::Result<Vec<AuditRecord>> {
    let path = directory.join("audit.jsonl");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    fs::read_to_string(path)?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(io::Error::other))
        .collect()
}

fn append_audit(directory: &std::path::Path, payload: AuditPayload) -> io::Result<()> {
    let records = read_audit(directory)?;
    verify_audit(directory, &records)?;
    let previous_mac = records
        .last()
        .map_or_else(String::new, |record| record.mac.clone());
    let sequence = records.len() as u64;
    let bytes =
        serde_json::to_vec(&(sequence, &previous_mac, &payload)).map_err(io::Error::other)?;
    let mut message = b"solmu-boxer-supervisor-audit-v1\0".to_vec();
    message.extend_from_slice(&bytes);
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key(directory)?);
    let record = AuditRecord {
        sequence,
        previous_mac,
        payload,
        mac: hex::encode(ring::hmac::sign(&key, &message).as_ref()),
    };
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(directory.join("audit.jsonl"))?;
    serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()
}

fn verify_audit(directory: &std::path::Path, records: &[AuditRecord]) -> io::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key(directory)?);
    let mut previous = String::new();
    for (index, record) in records.iter().enumerate() {
        if record.sequence != index as u64 || record.previous_mac != previous {
            return Err(io::Error::other(
                "Supervisor audit sequence or link is invalid",
            ));
        }
        let bytes = serde_json::to_vec(&(record.sequence, &record.previous_mac, &record.payload))
            .map_err(io::Error::other)?;
        let mut message = b"solmu-boxer-supervisor-audit-v1\0".to_vec();
        message.extend_from_slice(&bytes);
        let mac = hex::decode(&record.mac).map_err(io::Error::other)?;
        ring::hmac::verify(&key, &message, &mac)
            .map_err(|_| io::Error::other("Supervisor audit authentication failed"))?;
        previous = record.mac.clone();
    }
    Ok(())
}

fn validate_id(id: &str) -> io::Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| io::Error::other("Invalid Boxer supervisor ID"))
}

fn text<'a>(argument: &'a OsString, name: &str) -> io::Result<&'a str> {
    argument
        .to_str()
        .ok_or_else(|| io::Error::other(format!("Invalid {name}")))
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Once => "once",
        Decision::Session => "for this session",
        Decision::Deny => "denied",
    }
}

fn usage() -> io::Error {
    io::Error::other(
        "Usage: boxer supervisor SESSION_ID list [--json] | history [--json] | approve REQUEST_ID --once | approve REQUEST_ID --session [--host DOMAIN_PATTERN[:PORT]] | deny REQUEST_ID",
    )
}
