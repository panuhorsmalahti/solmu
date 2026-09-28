use crate::api::state::Change;
use serde::Serialize;
use serde_yaml_ng::Value;
use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{Mutex, broadcast};

const MAX_SKILLS: usize = 128;
const FILE_LIMIT: u64 = 1_000_000;

#[derive(Clone, Serialize)]
pub struct Skill {
    name: String,
    description: String,
    path: String,
    compatibility: Option<String>,
    license: Option<String>,
    allowed_tools: Option<String>,
    metadata: BTreeMap<String, String>,
}
#[derive(Clone, Serialize)]
pub struct Issue {
    path: String,
    message: String,
}
#[derive(Clone, Serialize)]
pub struct Catalog {
    directory: String,
    items: Vec<Skill>,
    issues: Vec<Issue>,
}
#[derive(Clone)]
pub struct Snapshot {
    pub catalog: Catalog,
    pub roots: Vec<PathBuf>,
    fingerprint: u64,
}
impl Snapshot {
    pub fn context(&self) -> String {
        let items: Vec<_> = self
            .catalog
            .items
            .iter()
            .map(|skill| {
                serde_json::json!({
                    "name": skill.name, "description": skill.description,
                    "path": skill.path, "compatibility": skill.compatibility,
                })
            })
            .collect();
        format!(
            "Workspace skills discovered for this conversation:\n{}",
            serde_json::json!({"directory": self.catalog.directory, "skills": items})
        )
    }
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("{field} must be a string"))
}
fn optional(value: &Value, field: &str) -> Result<Option<String>, String> {
    if value
        .as_mapping()
        .is_some_and(|map| map.contains_key(Value::String(field.into())))
    {
        string(value, field).map(|text| Some(text.to_owned()))
    } else {
        Ok(None)
    }
}
pub(crate) fn parse(source: &str, directory: &str, path: String) -> Result<Skill, String> {
    let mut lines = source.trim_start_matches('\u{feff}').split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Err("SKILL.md must start with YAML frontmatter (---)".into());
    }
    let mut frontmatter = String::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        frontmatter.push_str(line);
    }
    if !closed {
        return Err("YAML frontmatter must end with --- on its own line".into());
    }
    let value: Value =
        serde_yaml_ng::from_str(&frontmatter).map_err(|error| format!("Invalid YAML: {error}"))?;
    if !value.is_mapping() {
        return Err("YAML frontmatter must be a mapping".into());
    }
    let name = string(&value, "name")?;
    if name.is_empty()
        || name.chars().count() > 64
        || !name
            .chars()
            .all(|character| character == '-' || character.is_lowercase() || character.is_numeric())
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
    {
        return Err("name must be 1–64 lowercase letters, numbers, or single hyphens".into());
    }
    if name != directory {
        return Err("name must match the skill directory name".into());
    }
    let description = string(&value, "description")?;
    if description.trim().is_empty() || description.chars().count() > 1024 {
        return Err("description must contain 1–1024 characters".into());
    }
    let compatibility = optional(&value, "compatibility")?;
    if compatibility
        .as_ref()
        .is_some_and(|text| text.trim().is_empty() || text.chars().count() > 500)
    {
        return Err("compatibility must contain 1–500 characters".into());
    }
    let mut metadata = BTreeMap::new();
    if let Some(mapping) = value
        .as_mapping()
        .and_then(|map| map.get(Value::String("metadata".into())))
    {
        for (key, value) in mapping
            .as_mapping()
            .ok_or("metadata must be a string-to-string mapping")?
        {
            metadata.insert(
                key.as_str()
                    .ok_or("metadata keys must be strings")?
                    .to_owned(),
                value
                    .as_str()
                    .ok_or("metadata values must be strings")?
                    .to_owned(),
            );
        }
    }
    Ok(Skill {
        name: name.into(),
        description: description.into(),
        path,
        compatibility,
        license: optional(&value, "license")?,
        allowed_tools: optional(&value, "allowed-tools")?,
        metadata,
    })
}

fn discover(workspace: &Path) -> Snapshot {
    let directory = workspace.join(".agents/skills");
    let mut snapshot = Snapshot {
        catalog: Catalog {
            directory: directory.to_string_lossy().into_owned(),
            items: Vec::new(),
            issues: Vec::new(),
        },
        roots: Vec::new(),
        fingerprint: 0,
    };
    let mut fingerprint = DefaultHasher::new();
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return snapshot,
        Err(error) => {
            snapshot.catalog.issues.push(Issue {
                path: snapshot.catalog.directory.clone(),
                message: format!("Cannot read skills directory: {error}"),
            });
            error.to_string().hash(&mut fingerprint);
            snapshot.fingerprint = fingerprint.finish();
            return snapshot;
        }
    };
    let mut entries: Vec<_> = entries.collect();
    entries.sort_by_key(|entry| {
        entry
            .as_ref()
            .map(|entry| entry.file_name())
            .unwrap_or_default()
    });
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                snapshot.catalog.issues.push(Issue {
                    path: snapshot.catalog.directory.clone(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        if !entry.path().is_dir() && !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!(".agents/skills/{name}/SKILL.md");
        let result: Result<(Skill, PathBuf), String> = (|| {
            if snapshot.catalog.items.len() >= MAX_SKILLS {
                return Err("Only the first 128 valid skills are loaded".into());
            }
            let root = entry
                .path()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let file = root
                .join("SKILL.md")
                .canonicalize()
                .map_err(|error| format!("Cannot read SKILL.md: {error}"))?;
            if !file.starts_with(&root) {
                return Err("SKILL.md must stay inside its skill directory".into());
            }
            let mut source = String::new();
            fs::File::open(&file)
                .map_err(|error| error.to_string())?
                .take(FILE_LIMIT + 1)
                .read_to_string(&mut source)
                .map_err(|error| format!("Cannot read UTF-8 SKILL.md: {error}"))?;
            if source.len() as u64 > FILE_LIMIT {
                return Err("SKILL.md exceeds the 1 MB limit".into());
            }
            path.hash(&mut fingerprint);
            root.hash(&mut fingerprint);
            source.hash(&mut fingerprint);
            Ok((parse(&source, &name, path.clone())?, root))
        })();
        match result {
            Ok((skill, root)) => {
                snapshot.catalog.items.push(skill);
                snapshot.roots.push(root);
            }
            Err(message) => {
                path.hash(&mut fingerprint);
                message.hash(&mut fingerprint);
                snapshot.catalog.issues.push(Issue { path, message });
            }
        }
    }
    snapshot.fingerprint = fingerprint.finish();
    snapshot
}

async fn scan(workspace: PathBuf) -> Snapshot {
    tokio::task::spawn_blocking(move || {
        let mut snapshot = discover(&workspace);
        let plugins = crate::plugins::discover(&workspace);
        for (skill, root) in plugins.skills {
            if snapshot.catalog.items.len() >= MAX_SKILLS {
                break;
            }
            snapshot.catalog.items.push(skill);
            snapshot.roots.push(root);
        }
        let mut hash = DefaultHasher::new();
        snapshot.fingerprint.hash(&mut hash);
        plugins.fingerprint.hash(&mut hash);
        snapshot.fingerprint = hash.finish();
        snapshot
    })
    .await
    .expect("skills discovery task")
}
struct Watched {
    threads: BTreeSet<String>,
    snapshot: Snapshot,
}
#[derive(Clone)]
pub struct Manager {
    watched: Arc<Mutex<BTreeMap<PathBuf, Watched>>>,
    events: broadcast::Sender<Change>,
}
impl Manager {
    pub fn new(events: broadcast::Sender<Change>) -> Self {
        let watched: Arc<Mutex<BTreeMap<PathBuf, Watched>>> = Arc::default();
        let weak = Arc::downgrade(&watched);
        let changes = events.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let Some(watched) = weak.upgrade() else {
                    return;
                };
                let mut watched = watched.lock().await;
                for (workspace, entry) in watched.iter_mut() {
                    let snapshot = scan(workspace.clone()).await;
                    if snapshot.fingerprint != entry.snapshot.fingerprint {
                        for id in &entry.threads {
                            let _ = changes.send(Change::skills(id));
                        }
                        entry.snapshot = snapshot;
                    }
                }
            }
        });
        Self { watched, events }
    }
    pub async fn load(&self, thread: &str, workspace: PathBuf) -> Snapshot {
        let mut watched = self.watched.lock().await;
        let snapshot = scan(workspace.clone()).await;
        let entry = watched.entry(workspace).or_insert_with(|| Watched {
            threads: BTreeSet::new(),
            snapshot: snapshot.clone(),
        });
        entry.threads.insert(thread.into());
        if snapshot.fingerprint != entry.snapshot.fingerprint {
            for id in &entry.threads {
                let _ = self.events.send(Change::skills(id));
            }
        }
        entry.snapshot = snapshot.clone();
        snapshot
    }
    pub async fn forget(&self, thread: &str) {
        self.watched.lock().await.retain(|_, entry| {
            entry.threads.remove(thread);
            !entry.threads.is_empty()
        });
    }
}
