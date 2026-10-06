use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use walkdir::WalkDir;

pub const CHILD_ARGUMENT: &str = "--boxer-rollback-child";

#[cfg(unix)]
static INTERRUPTED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(unix)]
extern "C" fn interrupted(signal: i32) {
    INTERRUPTED.store(signal, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    Directory { mode: u32 },
    File { hash: String, mode: u32 },
    Symlink { target: String, directory: bool },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    root_mode: u32,
    entries: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Session {
    version: u32,
    id: String,
    created_unix_ms: u128,
    workspace: String,
    program: String,
    before: Snapshot,
    after: Option<Snapshot>,
    #[serde(default)]
    audit: Vec<AuditRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditPayload {
    event: String,
    timestamp_unix_ms: u128,
    workspace: String,
    program: String,
    snapshot_sha256: String,
    exit_code: Option<i32>,
    changes: Vec<AuditChange>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AuditChange {
    path: String,
    before_sha256: Option<String>,
    after_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    sequence: u64,
    previous_mac: String,
    payload: AuditPayload,
    mac: String,
}

pub fn command(arguments: &[OsString]) -> io::Result<i32> {
    match arguments.get(1).and_then(|value| value.to_str()) {
        Some("list") if arguments.len() == 2 => list(),
        Some("show") if (3..=4).contains(&arguments.len()) => {
            let diff = arguments.get(3).is_some_and(|arg| arg == "--diff");
            if arguments.len() == 4 && !diff {
                return Err(usage());
            }
            show(required_text(arguments, 2)?, diff)
        }
        Some("restore") if (3..=4).contains(&arguments.len()) => {
            let dry_run = arguments.get(3).is_some_and(|arg| arg == "--dry-run");
            if arguments.len() == 4 && !dry_run {
                return Err(usage());
            }
            restore(required_text(arguments, 2)?, dry_run)
        }
        Some("cleanup") if arguments.len() >= 3 => cleanup(&arguments[2..]),
        Some("audit") if arguments.len() == 3 => match required_text(arguments, 2)? {
            "list" => audit_list(),
            _ => Err(audit_usage()),
        },
        Some("audit") if (4..=5).contains(&arguments.len()) => match required_text(arguments, 2)? {
            "show" => audit_show(required_text(arguments, 3)?),
            "export" if arguments.len() == 4 => audit_export(required_text(arguments, 3)?),
            "verify" if arguments.len() == 4 => audit_verify(required_text(arguments, 3)?),
            _ => Err(audit_usage()),
        },
        _ => Err(usage()),
    }
}

pub fn run(arguments: &[OsString], workspace: &Path, program: &OsStr) -> io::Result<i32> {
    let store = store_root(workspace)?;
    let id = Uuid::new_v4().hyphenated().to_string();
    let created_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis();
    let workspace_text = workspace
        .to_str()
        .ok_or_else(|| io::Error::other("Rollback requires a Unicode workspace path"))?
        .to_owned();
    let before = capture(workspace, &store)?;
    let mut session = Session {
        version: 1,
        id: id.clone(),
        created_unix_ms,
        workspace: workspace_text,
        program: Path::new(program)
            .file_name()
            .unwrap_or(program)
            .to_string_lossy()
            .into_owned(),
        before,
        after: None,
        audit: Vec::new(),
    };
    let start_audit = AuditPayload {
        event: "session_started".into(),
        timestamp_unix_ms: now_ms()?,
        workspace: session.workspace.clone(),
        program: session.program.clone(),
        snapshot_sha256: snapshot_digest(&session.before)?,
        exit_code: None,
        changes: Vec::new(),
    };
    append_audit(&store, &mut session, start_audit)?;
    save_session(&store, &session)?;

    #[cfg(unix)]
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // The supervised Boxer child is exec'd with default signal dispositions;
        // this process records a terminal interrupt and stays alive to finish its snapshot.
        if unsafe { libc::signal(signal, interrupted as *const () as usize) } == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    let mut child_command = Command::new(std::env::current_exe()?);
    child_command
        .arg(CHILD_ARGUMENT)
        .args(arguments)
        .current_dir(workspace);
    let child_result = child_command.spawn().and_then(|mut child| {
        #[cfg(unix)]
        let mut forwarded = false;
        let status = loop {
            #[cfg(unix)]
            {
                let signal = INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed);
                if signal != 0 && !forwarded {
                    forwarded = true;
                    // The terminal normally signals the whole process group. Forward
                    // once in case the signal arrived while the child was starting.
                    unsafe {
                        libc::kill(child.id() as libc::pid_t, signal);
                    }
                }
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        Ok(status)
    });

    let after_result = capture(workspace, &store);
    match after_result {
        Ok(after) => {
            let delta = audit_changes(session.after.as_ref().unwrap_or(&session.before), &after)?;
            session.after = Some(after);
            let code = child_result
                .as_ref()
                .ok()
                .and_then(std::process::ExitStatus::code);
            let completion_audit = AuditPayload {
                event: "session_completed".into(),
                timestamp_unix_ms: now_ms()?,
                workspace: session.workspace.clone(),
                program: session.program.clone(),
                snapshot_sha256: snapshot_digest(
                    session.after.as_ref().expect("snapshot just assigned"),
                )?,
                exit_code: code,
                changes: delta,
            };
            append_audit(&store, &mut session, completion_audit)?;
            save_session(&store, &session)?;
        }
        Err(error) => {
            return Err(io::Error::other(format!(
                "Rollback session {id} started, but its final snapshot failed: {error}"
            )));
        }
    }
    eprintln!("Rollback session: {id}");
    let status = child_result?;
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Ok(status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)))
    }
    #[cfg(not(unix))]
    {
        Ok(status.code().unwrap_or(1))
    }
}

fn usage() -> io::Error {
    io::Error::other(
        "Usage: boxer rollback list | show <session-id> [--diff] | restore <session-id> [--dry-run] | cleanup [--older-than DAYS] [--keep COUNT] [--dry-run]",
    )
}

fn cleanup(arguments: &[OsString]) -> io::Result<i32> {
    let mut older_than_days = None;
    let mut keep = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].to_str() {
            Some("--dry-run") if !dry_run => {
                dry_run = true;
                index += 1;
            }
            Some("--older-than") if older_than_days.is_none() && index + 1 < arguments.len() => {
                older_than_days = Some(
                    arguments[index + 1]
                        .to_str()
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|days| *days > 0)
                        .ok_or_else(usage)?,
                );
                index += 2;
            }
            Some("--keep") if keep.is_none() && index + 1 < arguments.len() => {
                keep = Some(
                    arguments[index + 1]
                        .to_str()
                        .and_then(|value| value.parse::<usize>().ok())
                        .ok_or_else(usage)?,
                );
                index += 2;
            }
            _ => return Err(usage()),
        }
    }
    if older_than_days.is_none() && keep.is_none() {
        return Err(usage());
    }
    let store = store_root_for_commands()?;
    let mut sessions = load_sessions(&store)?;
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_unix_ms));
    let now = now_ms()?;
    let cutoff =
        older_than_days.map(|days| now.saturating_sub(u128::from(days).saturating_mul(86_400_000)));
    let mut removed = Vec::new();
    let mut retained = Vec::new();
    for (position, session) in sessions.into_iter().enumerate() {
        let expired = cutoff.is_some_and(|cutoff| session.created_unix_ms < cutoff);
        let beyond_limit = keep.is_some_and(|limit| position >= limit);
        if expired || beyond_limit {
            removed.push(session);
        } else {
            retained.push(session);
        }
    }
    let referenced = session_blobs(&retained)?;
    let blob_dir = store.join("blobs");
    let mut unreferenced = Vec::new();
    for entry in fs::read_dir(&blob_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_hash(&name) || referenced.contains(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() {
            return Err(io::Error::other(
                "Rollback blob store contains a non-file object",
            ));
        }
        unreferenced.push((entry.path(), metadata.len()));
    }
    let reclaimed = unreferenced.iter().map(|(_, size)| *size).sum::<u64>();
    println!("Sessions to remove: {}", removed.len());
    for session in &removed {
        println!("{}\t{}\t{}", session.id, session.program, session.workspace);
    }
    println!(
        "Unreferenced snapshot objects: {} ({} bytes)",
        unreferenced.len(),
        reclaimed
    );
    if dry_run {
        println!("Dry run: no rollback data removed");
        return Ok(0);
    }
    for session in &removed {
        fs::remove_file(store.join("sessions").join(format!("{}.json", session.id)))?;
    }
    for (path, _) in unreferenced {
        fs::remove_file(path)?;
    }
    println!("Cleanup complete");
    Ok(0)
}

fn session_blobs(sessions: &[Session]) -> io::Result<HashSet<String>> {
    let mut hashes = HashSet::new();
    for session in sessions {
        for snapshot in std::iter::once(&session.before).chain(session.after.iter()) {
            for entry in snapshot.entries.values() {
                if let Entry::File { hash, .. } = entry {
                    if !valid_hash(hash) {
                        return Err(io::Error::other(
                            "Rollback session contains an invalid content hash",
                        ));
                    }
                    hashes.insert(hash.clone());
                }
            }
        }
    }
    Ok(hashes)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn audit_usage() -> io::Error {
    io::Error::other(
        "Usage: boxer rollback audit list | show <session-id> | export <session-id> | verify <session-id>",
    )
}

fn now_ms() -> io::Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis())
}

fn key(store: &Path) -> io::Result<Vec<u8>> {
    let path = store.join("audit.key");
    if !path.exists() {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
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
        return Err(io::Error::other("Invalid Boxer audit key"));
    }
    Ok(bytes)
}

fn append_audit(store: &Path, session: &mut Session, payload: AuditPayload) -> io::Result<()> {
    let sequence = session.audit.len() as u64;
    let previous_mac = session
        .audit
        .last()
        .map(|item| item.mac.clone())
        .unwrap_or_default();
    let bytes =
        serde_json::to_vec(&(sequence, &previous_mac, &payload)).map_err(io::Error::other)?;
    let mut message = b"solmu-boxer-audit-v1\0".to_vec();
    message.extend_from_slice(&bytes);
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key(store)?);
    let mac = ring::hmac::sign(&key, &message);
    session.audit.push(AuditRecord {
        sequence,
        previous_mac,
        payload,
        mac: hex::encode(mac.as_ref()),
    });
    save_session(store, session)
}

fn audit_changes(before: &Snapshot, after: &Snapshot) -> io::Result<Vec<AuditChange>> {
    let mut result = Vec::new();
    for (kind, path) in changes(before, after)? {
        let encoded = encode_path(&path);
        let before_entry = before.entries.get(&encoded);
        let after_entry = after.entries.get(&encoded);
        let digest = |entry: Option<&Entry>| -> io::Result<Option<String>> {
            entry
                .map(|entry| {
                    serde_json::to_vec(entry)
                        .map(|bytes| hex::encode(Sha256::digest(bytes)))
                        .map_err(io::Error::other)
                })
                .transpose()
        };
        let _ = kind;
        result.push(AuditChange {
            path: display_path(&path),
            before_sha256: digest(before_entry)?,
            after_sha256: digest(after_entry)?,
        });
    }
    Ok(result)
}

fn snapshot_digest(snapshot: &Snapshot) -> io::Result<String> {
    let bytes = serde_json::to_vec(snapshot).map_err(io::Error::other)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn verify_audit(store: &Path, session: &Session) -> io::Result<()> {
    if session.audit.is_empty() {
        return Err(io::Error::other("This session has no audit records"));
    }
    let key = key(store)?;
    let mut previous = String::new();
    for (index, record) in session.audit.iter().enumerate() {
        if record.sequence != index as u64 || record.previous_mac != previous {
            return Err(io::Error::other("Audit chain sequence or link is invalid"));
        }
        let bytes = serde_json::to_vec(&(record.sequence, &record.previous_mac, &record.payload))
            .map_err(io::Error::other)?;
        let expected = hex::decode(&record.mac).map_err(io::Error::other)?;
        let mut message = b"solmu-boxer-audit-v1\0".to_vec();
        message.extend_from_slice(&bytes);
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key);
        ring::hmac::verify(&key, &message, &expected)
            .map_err(|_| io::Error::other("Audit record authentication failed"))?;
        previous = record.mac.clone();
    }
    let first = &session.audit[0].payload;
    if first.event != "session_started"
        || first.workspace != session.workspace
        || first.program != session.program
        || first.snapshot_sha256 != snapshot_digest(&session.before)?
    {
        return Err(io::Error::other(
            "Audit start record does not match the session manifest",
        ));
    }
    if let Some(after) = &session.after {
        let last = &session.audit[session.audit.len() - 1].payload;
        if last.event != "session_completed"
            || last.workspace != session.workspace
            || last.program != session.program
            || last.snapshot_sha256 != snapshot_digest(after)?
            || last.changes != audit_changes(&session.before, after)?
        {
            return Err(io::Error::other(
                "Audit completion record does not match the session snapshot",
            ));
        }
    }
    Ok(())
}

fn audit_list() -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let mut sessions = load_sessions(&store)?;
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_unix_ms));
    println!("SESSION\tEVENTS\tSTATUS");
    for session in sessions {
        let status = if session.audit.is_empty() {
            "not audited"
        } else if verify_audit(&store, &session).is_ok() {
            "verified"
        } else {
            "invalid"
        };
        println!("{}\t{}\t{}", session.id, session.audit.len(), status);
    }
    Ok(0)
}
fn audit_show(id: &str) -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let session = read_session(&store, id)?;
    println!("Audit for {} ({} events)", id, session.audit.len());
    for record in session.audit {
        println!(
            "{}\t{}\t{}",
            record.sequence, record.payload.event, record.payload.timestamp_unix_ms
        );
    }
    Ok(0)
}
fn audit_verify(id: &str) -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let session = read_session(&store, id)?;
    verify_audit(&store, &session)?;
    println!("Audit verified: {} ({} events)", id, session.audit.len());
    Ok(0)
}

fn audit_export(id: &str) -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let session = read_session(&store, id)?;
    verify_audit(&store, &session)?;
    let export = serde_json::json!({
        "version": 1,
        "session_id": session.id,
        "created_unix_ms": session.created_unix_ms,
        "workspace": session.workspace,
        "program": session.program,
        "verified": true,
        "events": session.audit,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&export).map_err(io::Error::other)?
    );
    Ok(0)
}

fn required_text(arguments: &[OsString], index: usize) -> io::Result<&str> {
    arguments
        .get(index)
        .and_then(|value| value.to_str())
        .ok_or_else(usage)
}

fn store_root(workspace: &Path) -> io::Result<PathBuf> {
    let root = if let Some(path) = std::env::var_os("BOXER_ROLLBACK_DIR") {
        PathBuf::from(path)
    } else {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("Cannot find the home directory for rollback data"))?;
        home.join(".boxer").join("rollback")
    };
    let absolute = normalize_absolute(&root)?;
    if possible_canonical_path(&absolute)?.starts_with(workspace) {
        return Err(io::Error::other(format!(
            "Rollback storage ({}) must be outside the workspace; set BOXER_ROLLBACK_DIR to another directory",
            absolute.display()
        )));
    }
    fs::create_dir_all(root.join("sessions"))?;
    fs::create_dir_all(root.join("blobs"))?;
    let root = root.canonicalize()?;
    for name in ["sessions", "blobs"] {
        let directory = root.join(name).canonicalize()?;
        if !directory.starts_with(&root) || directory.starts_with(workspace) {
            return Err(io::Error::other(format!(
                "Rollback storage ({}) must stay outside the workspace; set BOXER_ROLLBACK_DIR to another directory",
                directory.display()
            )));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(root.join("sessions"), fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(root.join("blobs"), fs::Permissions::from_mode(0o700))?;
    }
    Ok(root)
}

fn normalize_absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    Ok(normalized)
}

fn possible_canonical_path(path: &Path) -> io::Result<PathBuf> {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| io::Error::other("Rollback storage has no existing parent"))?;
        suffix.push(name.to_owned());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| io::Error::other("Rollback storage has no existing parent"))?;
    }
    let mut canonical = ancestor.canonicalize()?;
    for component in suffix.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn capture(workspace: &Path, store: &Path) -> io::Result<Snapshot> {
    let root_metadata = fs::symlink_metadata(workspace)?;
    let mut entries = BTreeMap::new();
    for item in WalkDir::new(workspace)
        .follow_links(false)
        .sort_by_file_name()
        .min_depth(1)
    {
        let item = item.map_err(io::Error::other)?;
        let path = item.path();
        let relative = path.strip_prefix(workspace).map_err(io::Error::other)?;
        let key = encode_path(relative);
        let file_type = item.file_type();
        let entry = if file_type.is_symlink() {
            let target = fs::read_link(path)?;
            Entry::Symlink {
                target: encode_path(&target),
                directory: fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()),
            }
        } else if file_type.is_dir() {
            Entry::Directory {
                mode: file_mode(&item.metadata().map_err(io::Error::other)?),
            }
        } else if file_type.is_file() {
            Entry::File {
                hash: store_blob(path, store)?,
                mode: file_mode(&item.metadata().map_err(io::Error::other)?),
            }
        } else {
            return Err(io::Error::other(format!(
                "Rollback cannot snapshot special file {}",
                path.display()
            )));
        };
        entries.insert(key, entry);
    }
    Ok(Snapshot {
        root_mode: file_mode(&root_metadata),
        entries,
    })
}

fn store_blob(source: &Path, store: &Path) -> io::Result<String> {
    let mut input = File::open(source)?;
    let temporary = tempfile::NamedTempFile::new_in(store.join("blobs"))?;
    let mut output = temporary.as_file();
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    output.sync_all()?;
    let hash = hex::encode(hasher.finalize());
    let destination = store.join("blobs").join(&hash);
    match fs::hard_link(temporary.path(), &destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if hash_file(&destination)? != hash {
                return Err(io::Error::other(format!(
                    "Rollback content store is corrupt at {}",
                    destination.display()
                )));
            }
        }
        Err(error) => return Err(error),
    }
    Ok(hash)
}

fn hash_file(path: &Path) -> io::Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::other(format!(
            "Rollback content object is not a regular file: {}",
            path.display()
        )));
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn save_session(store: &Path, session: &Session) -> io::Result<()> {
    let destination = store.join("sessions").join(format!("{}.json", session.id));
    let temporary = tempfile::NamedTempFile::new_in(store.join("sessions"))?;
    serde_json::to_writer(temporary.as_file(), session).map_err(io::Error::other)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn read_session(store: &Path, id: &str) -> io::Result<Session> {
    let parsed =
        Uuid::parse_str(id).map_err(|_| io::Error::other("Invalid rollback session id"))?;
    let id = parsed.hyphenated().to_string();
    let path = store.join("sessions").join(format!("{id}.json"));
    let metadata = fs::metadata(&path)?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err(io::Error::other("Rollback session metadata is too large"));
    }
    let session: Session = serde_json::from_reader(File::open(path)?).map_err(io::Error::other)?;
    if session.version != 1 || session.id != id {
        return Err(io::Error::other(
            "Unsupported or mismatched rollback session",
        ));
    }
    Ok(session)
}

fn list() -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let mut sessions = load_sessions(&store)?;
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_unix_ms));
    println!("SESSION\tPROGRAM\tCHANGED\tWORKSPACE");
    for session in sessions {
        let changed = session
            .after
            .as_ref()
            .map(|after| changes(&session.before, after).map(|entries| entries.len().to_string()))
            .transpose()?
            .unwrap_or_else(|| "pending".into());
        println!(
            "{}\t{}\t{}\t{}",
            session.id, session.program, changed, session.workspace
        );
    }
    Ok(0)
}

fn show(id: &str, print_diff: bool) -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let session = read_session(&store, id)?;
    println!("Session: {}", session.id);
    println!("Program: {}", session.program);
    println!("Workspace: {}", session.workspace);
    println!("Started: {} ms since Unix epoch", session.created_unix_ms);
    let Some(after) = &session.after else {
        println!("Snapshot: incomplete");
        return Ok(0);
    };
    let changes = changes(&session.before, after)?;
    println!("Paths changed: {}", changes.len());
    if print_diff {
        for (kind, path) in changes {
            println!("{kind}\t{}", display_path(&path));
        }
    }
    Ok(0)
}

fn changes(before: &Snapshot, after: &Snapshot) -> io::Result<Vec<(&'static str, PathBuf)>> {
    let keys: HashSet<_> = before
        .entries
        .keys()
        .chain(after.entries.keys())
        .cloned()
        .collect();
    let mut result = keys
        .into_iter()
        .filter_map(|key| {
            let old = before.entries.get(&key);
            let new = after.entries.get(&key);
            if old == new {
                return None;
            }
            let kind = match (old, new) {
                (None, Some(_)) => "added",
                (Some(_), None) => "deleted",
                _ => "modified",
            };
            Some(decode_path(&key).map(|path| (kind, path)))
        })
        .collect::<io::Result<Vec<_>>>()?;
    if before.root_mode != after.root_mode {
        result.push(("modified", PathBuf::from(".")));
    }
    result.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(result)
}

fn restore(id: &str, dry_run: bool) -> io::Result<i32> {
    let store = store_root_for_commands()?;
    let session = read_session(&store, id)?;
    let workspace = PathBuf::from(&session.workspace);
    let changed = if let Some(after) = &session.after {
        changes(&session.before, after)?
    } else {
        session
            .before
            .entries
            .keys()
            .map(|path| decode_path(path).map(|path| ("restore", path)))
            .collect::<io::Result<Vec<_>>>()?
    };
    if dry_run {
        println!(
            "Would restore pre-session contents of {} ({} recorded paths){}",
            session.workspace,
            changed.len(),
            if session.after.is_none() {
                " (final snapshot missing)"
            } else {
                ""
            }
        );
        for (kind, path) in changed {
            println!("{kind}\t{}", display_path(&path));
        }
        return Ok(0);
    }
    if workspace.exists() {
        let current = workspace.canonicalize()?;
        if current != workspace {
            return Err(io::Error::other(
                "Rollback workspace path changed since the session; refusing to restore",
            ));
        }
    } else if !workspace.parent().is_some_and(|parent| parent.is_dir()) {
        return Err(io::Error::other(
            "Rollback workspace and its parent no longer exist",
        ));
    }
    restore_snapshot(&session.before, &workspace, &store)?;
    println!("Restored workspace to its pre-session snapshot");
    Ok(0)
}

fn restore_snapshot(snapshot: &Snapshot, workspace: &Path, store: &Path) -> io::Result<()> {
    validate_snapshot(snapshot)?;
    let parent = workspace
        .parent()
        .ok_or_else(|| io::Error::other("Cannot restore a filesystem root"))?;
    let stage = tempfile::Builder::new()
        .prefix(".boxer-restore-")
        .tempdir_in(parent)?;
    populate(stage.path(), snapshot, store)?;
    let stage = stage.keep();
    let name = workspace
        .file_name()
        .ok_or_else(|| io::Error::other("Workspace has no final path component"))?;
    let backup = parent.join(format!(
        ".{}.boxer-backup-{}",
        name.to_string_lossy(),
        Uuid::new_v4()
    ));
    std::env::set_current_dir(parent)?;
    let had_workspace = match fs::symlink_metadata(workspace) {
        Ok(_) => {
            if let Err(error) = fs::rename(workspace, &backup) {
                let _ = fs::remove_dir_all(&stage);
                return Err(error);
            }
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&stage, workspace) {
        let recovery = if had_workspace {
            fs::rename(&backup, workspace)
        } else {
            Ok(())
        };
        let _ = fs::remove_dir_all(&stage);
        return match recovery {
            Ok(()) => Err(error),
            Err(recovery_error) => Err(io::Error::other(format!(
                "Restore failed ({error}) and workspace recovery failed ({recovery_error}); original files remain at {}",
                backup.display()
            ))),
        };
    }
    std::env::set_current_dir(workspace)?;
    if had_workspace {
        fs::remove_dir_all(&backup).map_err(|error| {
            io::Error::other(format!(
                "Workspace was restored but the temporary backup {} could not be removed: {error}",
                backup.display()
            ))
        })?;
    }
    Ok(())
}

fn validate_snapshot(snapshot: &Snapshot) -> io::Result<Vec<(PathBuf, Entry)>> {
    let mut entries = Vec::with_capacity(snapshot.entries.len());
    for (encoded, entry) in &snapshot.entries {
        let relative = decode_path(encoded)?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(io::Error::other("Invalid path in rollback snapshot"));
        }
        if let Entry::Symlink { target, .. } = entry {
            decode_path(target)?;
        }
        if let Entry::File { hash, .. } = entry
            && (hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(io::Error::other(
                "Invalid content hash in rollback snapshot",
            ));
        }
        entries.push((relative, entry.clone()));
    }
    for (relative, _) in &entries {
        let mut parent = relative.parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            if entries.iter().any(|(candidate, entry)| {
                candidate == path && !matches!(entry, Entry::Directory { .. })
            }) {
                return Err(io::Error::other(
                    "A rollback path is nested beneath a file or symbolic link",
                ));
            }
            parent = path.parent();
        }
    }
    Ok(entries)
}

fn populate(root: &Path, snapshot: &Snapshot, store: &Path) -> io::Result<()> {
    let mut entries = validate_snapshot(snapshot)?;
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.1, Entry::Directory { .. });
        let right_dir = matches!(right.1, Entry::Directory { .. });
        right_dir.cmp(&left_dir).then_with(|| {
            left.0
                .components()
                .count()
                .cmp(&right.0.components().count())
        })
    });
    for (relative, entry) in &entries {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        match entry {
            Entry::Directory { .. } => fs::create_dir_all(&destination)?,
            Entry::File { hash, mode } => {
                let source = store.join("blobs").join(hash);
                if hash_file(&source)? != *hash {
                    return Err(io::Error::other(format!(
                        "Rollback content verification failed for {hash}"
                    )));
                }
                let mut input = File::open(source)?;
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                set_file_mode(&destination, *mode)?;
            }
            Entry::Symlink { target, directory } => {
                let target = decode_path(target)?;
                create_symlink(&target, &destination, *directory)?;
            }
        }
    }
    for (relative, entry) in entries.iter().rev() {
        if let Entry::Directory { mode } = entry {
            set_file_mode(&root.join(relative), *mode)?;
        }
    }
    set_file_mode(root, snapshot.root_mode)
}

fn load_sessions(store: &Path) -> io::Result<Vec<Session>> {
    let directory = store.join("sessions");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            let id = entry
                .path()
                .file_stem()
                .and_then(OsStr::to_str)
                .ok_or_else(|| io::Error::other("Invalid rollback session filename"))?
                .to_owned();
            sessions.push(read_session(store, &id)?);
        }
    }
    Ok(sessions)
}

fn store_root_for_commands() -> io::Result<PathBuf> {
    let empty_workspace = Path::new("/__boxer_store_check_only__");
    let root = store_root(empty_workspace)?;
    Ok(root)
}

fn file_mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.mode() & 0o7777
    }
    #[cfg(windows)]
    {
        if metadata.permissions().readonly() {
            0o444
        } else {
            0o666
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        0o666
    }
}

fn set_file_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(mode & 0o222 == 0);
        fs::set_permissions(path, permissions)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = mode;
        Ok(())
    }
}

fn encode_path(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        hex::encode(path.as_os_str().as_bytes())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let bytes = path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        hex::encode(bytes)
    }
    #[cfg(not(any(unix, windows)))]
    {
        hex::encode(path.to_string_lossy().as_bytes())
    }
}

fn decode_path(encoded: &str) -> io::Result<PathBuf> {
    let bytes = hex::decode(encoded).map_err(io::Error::other)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(OsString::from_vec(bytes)))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if bytes.len() % 2 != 0 {
            return Err(io::Error::other("Invalid Windows path in rollback data"));
        }
        Ok(PathBuf::from(OsString::from_wide(
            &bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|chunk| u16::from_le_bytes(*chunk))
                .collect::<Vec<_>>(),
        )))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(PathBuf::from(
            String::from_utf8(bytes).map_err(io::Error::other)?,
        ))
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, destination: &Path, _: bool) -> io::Result<()> {
    std::os::unix::fs::symlink(target, destination)
}

#[cfg(windows)]
fn create_symlink(target: &Path, destination: &Path, directory: bool) -> io::Result<()> {
    if directory {
        std::os::windows::fs::symlink_dir(target, destination)
    } else {
        std::os::windows::fs::symlink_file(target, destination)
    }
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_: &Path, _: &Path, _: bool) -> io::Result<()> {
    Err(io::Error::other(
        "Symbolic link restore is unsupported on this OS",
    ))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
