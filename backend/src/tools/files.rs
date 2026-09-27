use super::{AgentTool, Context, FILE_LIMIT, OUTPUT_LIMIT, definition};
use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Arc};

#[derive(Clone, Copy)]
enum FileTool {
    Read,
    Write,
    Edit,
    Glob,
    Grep,
}

pub fn builtins() -> Vec<Arc<dyn AgentTool>> {
    [
        FileTool::Read,
        FileTool::Write,
        FileTool::Edit,
        FileTool::Glob,
        FileTool::Grep,
    ]
    .into_iter()
    .map(|tool| Arc::new(tool) as Arc<dyn AgentTool>)
    .collect()
}

impl AgentTool for FileTool {
    fn definition(&self) -> Tool {
        let path = json!({"type":"string","description":"Path relative to the thread workspace"});
        match self {
            Self::Read => definition(
                "Read",
                "Read a UTF-8 file in the workspace. Lines start at 1.",
                json!({"path":path,"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":2000}}),
                &["path"],
            ),
            Self::Write => definition(
                "Write",
                "Create or replace a UTF-8 file in the workspace.",
                json!({"path":path,"content":{"type":"string"}}),
                &["path", "content"],
            ),
            Self::Edit => definition(
                "Edit",
                "Replace exact text in a workspace file. By default the old text must occur exactly once.",
                json!({"path":path,"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"}}),
                &["path", "old_string", "new_string"],
            ),
            Self::Glob => definition(
                "Glob",
                "Find files by a glob relative to a workspace directory. Supports **, *, ?, and character classes.",
                json!({"pattern":{"type":"string"},"path":path}),
                &["pattern"],
            ),
            Self::Grep => definition(
                "Grep",
                "Search workspace UTF-8 files with a regular expression, returning paths, line numbers, and matching lines.",
                json!({"pattern":{"type":"string"},"path":path,"case_sensitive":{"type":"boolean"}}),
                &["pattern"],
            ),
        }
    }
    fn execute(
        &self,
        arguments: Value,
        context: Context,
    ) -> BoxFuture<'static, Result<Value, String>> {
        let tool = *self;
        Box::pin(async move {
            tokio::task::spawn_blocking(move || tool.run(arguments, &context))
                .await
                .map_err(|error| error.to_string())?
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindArgs {
    pattern: String,
    path: Option<String>,
    case_sensitive: Option<bool>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid tool arguments: {error}"))
}
fn read(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err("Path must be a regular file".into());
    }
    if fs::metadata(path).map_err(|error| error.to_string())?.len() > FILE_LIMIT {
        return Err("File exceeds the 1 MB limit".into());
    }
    // Bound the read too, in case the file grows after metadata is checked.
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("File exceeds the 1 MB limit".into());
    }
    String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text".into())
}
fn write(path: &Path, content: &str) -> Result<(), String> {
    if content.len() as u64 > FILE_LIMIT {
        return Err("Content exceeds the 1 MB limit".into());
    }
    let parent = path.parent().ok_or("Path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".solmu-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| error.to_string())
}
fn cancelled(context: &Context) -> Result<(), String> {
    if context.cancellation.is_cancelled() {
        Err("Tool cancelled".into())
    } else {
        Ok(())
    }
}
impl FileTool {
    fn run(self, arguments: Value, context: &Context) -> Result<Value, String> {
        cancelled(context)?;
        match self {
            Self::Read => {
                let args: ReadArgs = parse(arguments)?;
                let offset = args.offset.unwrap_or(1);
                let limit = args.limit.unwrap_or(200);
                if offset == 0 || !(1..=2000).contains(&limit) {
                    return Err("offset must be positive; limit must be between 1 and 2000".into());
                }
                let text = read(&context.read_path(&args.path)?)?;
                let lines: Vec<_> = text.lines().collect();
                let content = lines
                    .iter()
                    .skip(offset - 1)
                    .take(limit)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
                let truncated = content.len() > OUTPUT_LIMIT
                    || offset.saturating_sub(1).saturating_add(limit) < lines.len();
                Ok(
                    json!({"content":super::bounded(&content, OUTPUT_LIMIT),"offset":offset,"total_lines":lines.len(),"truncated":truncated}),
                )
            }
            Self::Write => {
                let args: WriteArgs = parse(arguments)?;
                let path = context.path(&args.path, true)?;
                cancelled(context)?;
                write(&path, &args.content)?;
                Ok(json!({"path":args.path,"bytes_written":args.content.len()}))
            }
            Self::Edit => {
                let args: EditArgs = parse(arguments)?;
                if args.old_string.is_empty() {
                    return Err("old_string must not be empty".into());
                }
                let path = context.path(&args.path, false)?;
                let content = read(&path)?;
                let count = content.matches(&args.old_string).count();
                if count == 0 {
                    return Err("old_string was not found".into());
                }
                if count > 1 && !args.replace_all {
                    return Err(
                        "old_string occurs more than once; provide more context or set replace_all"
                            .into(),
                    );
                }
                let updated = content.replace(&args.old_string, &args.new_string);
                cancelled(context)?;
                write(&path, &updated)?;
                Ok(json!({"path":args.path,"replacements":count}))
            }
            Self::Glob | Self::Grep => {
                let args: FindArgs = parse(arguments)?;
                let base = context.read_path(args.path.as_deref().unwrap_or("."))?;
                let glob = if matches!(self, Self::Glob) {
                    Some(glob::Pattern::new(&args.pattern).map_err(|error| error.to_string())?)
                } else {
                    None
                };
                let regex = if matches!(self, Self::Grep) {
                    Some(
                        regex::RegexBuilder::new(&args.pattern)
                            .case_insensitive(args.case_sensitive == Some(false))
                            .build()
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };
                let mut results = Vec::new();
                let mut bytes = 0;
                let mut truncated = false;
                'files: for (index, entry) in walkdir::WalkDir::new(&base)
                    .follow_links(false)
                    .into_iter()
                    .enumerate()
                {
                    cancelled(context)?;
                    if index >= 20_000 {
                        truncated = true;
                        break;
                    }
                    let entry = entry.map_err(|error| error.to_string())?;
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let path = entry.path();
                    let relative = path
                        .strip_prefix(&context.workspace)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if let Some(glob) = &glob {
                        let local = path
                            .strip_prefix(&base)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        if glob.matches_with(
                            &local,
                            glob::MatchOptions {
                                require_literal_separator: true,
                                ..Default::default()
                            },
                        ) {
                            bytes += relative.len();
                            if results.len() >= 500 || bytes > OUTPUT_LIMIT / 2 {
                                truncated = true;
                                break;
                            }
                            results.push(json!(relative));
                        }
                    } else if let Some(regex) = &regex {
                        let Ok(content) = read(path) else {
                            continue;
                        };
                        for (line, text) in content.lines().enumerate() {
                            if regex.is_match(text) {
                                let item = json!({"path":relative,"line":line+1,"text":super::bounded(text,2000)});
                                bytes += item.to_string().len();
                                if results.len() >= 200 || bytes > OUTPUT_LIMIT / 2 {
                                    truncated = true;
                                    break 'files;
                                }
                                results.push(item);
                            }
                        }
                    }
                }
                Ok(json!({"matches":results,"truncated":truncated}))
            }
        }
    }
}
