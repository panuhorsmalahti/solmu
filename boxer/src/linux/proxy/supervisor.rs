use crate::network::Target;
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
    time::{Duration, Instant},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Once,
    Session,
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
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

    pub fn request(&self, target: &Target) -> Decision {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let request = Request {
            id: id.clone(),
            target: target.clone(),
        };
        let request_path = self.directory.join("requests").join(format!("{id}.json"));
        let response_path = self.directory.join("responses").join(format!("{id}.json"));
        if write_new_json(&request_path, &request).is_err() {
            return Decision::Deny;
        }
        eprintln!(
            "Boxer approval needed for {}:{}; run `boxer supervisor {} list` in another terminal",
            target.host, target.port, self.id
        );
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let decision = loop {
            if !self.running.load(Ordering::Acquire) || Instant::now() >= deadline {
                break Decision::Deny;
            }
            match fs::read(&response_path) {
                Ok(contents) => match serde_json::from_slice::<Response>(&contents) {
                    Ok(response) => break response.decision,
                    Err(_) => thread::sleep(Duration::from_millis(25)),
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break Decision::Deny,
            }
        };
        let _ = fs::remove_file(request_path);
        let _ = fs::remove_file(response_path);
        decision
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.directory);
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
        "approve" if (4..=5).contains(&arguments.len()) => {
            let request_id = text(&arguments[3], "request ID")?;
            validate_id(request_id)?;
            let decision = match arguments.get(4).and_then(|argument| argument.to_str()) {
                None | Some("--once") => Decision::Once,
                Some("--session") => Decision::Session,
                _ => return Err(usage()),
            };
            respond(&directory, request_id, decision)?;
            println!(
                "Approved request {request_id} ({})",
                decision_name(decision)
            );
            Ok(0)
        }
        "deny" if arguments.len() == 4 => {
            let request_id = text(&arguments[3], "request ID")?;
            validate_id(request_id)?;
            respond(&directory, request_id, Decision::Deny)?;
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

fn respond(directory: &std::path::Path, id: &str, decision: Decision) -> io::Result<()> {
    let request = directory.join("requests").join(format!("{id}.json"));
    if !request.is_file() {
        return Err(io::Error::other("No such pending Boxer approval"));
    }
    write_new_json(
        &directory.join("responses").join(format!("{id}.json")),
        &Response { decision },
    )
}

fn write_new_json(path: &std::path::Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn directory(id: &str) -> io::Result<PathBuf> {
    validate_id(id)?;
    Ok(std::env::temp_dir().join(format!("boxer-supervisor-{id}")))
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
        "Usage: boxer supervisor SESSION_ID list [--json] | approve REQUEST_ID [--once|--session] | deny REQUEST_ID",
    )
}
