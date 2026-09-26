mod files;
mod shell;

use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub const OUTPUT_LIMIT: usize = 64_000;
pub const FILE_LIMIT: u64 = 1_000_000;

#[derive(Clone)]
pub struct Context {
    pub workspace: PathBuf,
    pub cancellation: CancellationToken,
}
impl Context {
    pub fn path(&self, requested: &str, create: bool) -> Result<PathBuf, String> {
        let requested = Path::new(requested);
        if requested
            .components()
            .any(|part| part == std::path::Component::ParentDir)
        {
            return Err("Parent-directory traversal is not allowed".into());
        }
        let target = if requested.is_absolute() {
            requested.to_owned()
        } else {
            self.workspace.join(requested)
        };
        let mut parent = target.as_path();
        while !parent.exists() {
            if !create {
                return Err("Path does not exist".into());
            }
            parent = parent.parent().ok_or("Path has no existing parent")?;
        }
        let resolved = parent.canonicalize().map_err(|error| error.to_string())?;
        if !resolved.starts_with(&self.workspace) {
            return Err("Path is outside the thread workspace".into());
        }
        if target.exists() {
            Ok(resolved)
        } else {
            Ok(resolved.join(
                target
                    .strip_prefix(parent)
                    .map_err(|error| error.to_string())?,
            ))
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ResultData {
    pub success: bool,
    pub output: Value,
}
impl ResultData {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: json!({"error":message.into()}),
        }
    }
}

/// Implement this interface to register additional tools. Provider schemas and
/// execution share the same registry; failed calls become model-visible results.
pub trait AgentTool: Send + Sync {
    fn definition(&self) -> Tool;
    fn execute(
        &self,
        arguments: Value,
        context: Context,
    ) -> BoxFuture<'static, Result<Value, String>>;
}
#[derive(Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn AgentTool>>,
}
impl Registry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: BTreeMap::new(),
        };
        for tool in files::builtins() {
            registry.register(tool);
        }
        registry.register(Arc::new(shell::Bash));
        registry
    }
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.definition().name.to_string(), tool);
    }
    pub fn definitions(&self) -> Vec<Tool> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }
    pub async fn execute(&self, name: &str, arguments: Value, context: Context) -> ResultData {
        let Some(tool) = self.tools.get(name) else {
            return ResultData::error(format!("Unknown tool: {name}"));
        };
        if context.cancellation.is_cancelled() {
            return ResultData::error("Tool cancelled");
        }
        let execution = tool.execute(arguments, context);
        // File mutations finish atomically once started. Dropping a blocking
        // task cannot stop its write; never report cancellation before it ends.
        let result = if name == "Bash" {
            tokio::time::timeout(std::time::Duration::from_secs(30), execution)
                .await
                .unwrap_or_else(|_| Err("Tool timed out after 30 seconds".into()))
        } else {
            execution.await
        };
        match result {
            Ok(output) => ResultData {
                success: output
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .is_none_or(|code| code == 0),
                output,
            },
            Err(error) => ResultData::error(error),
        }
    }
}

pub fn bounded(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn definition(name: &str, description: &str, properties: Value, required: &[&str]) -> Tool {
    Tool::new(name).with_description(description).with_schema(json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}))
}
