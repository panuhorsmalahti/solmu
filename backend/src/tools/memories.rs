use super::{AgentTool, Context, definition};
use crate::storage::memories;
use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Memory;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: String,
    id: Option<String>,
    content: Option<String>,
    query: Option<String>,
}

impl AgentTool for Memory {
    fn definition(&self) -> Tool {
        definition(
            "Memory",
            "Search, read, write, update, or delete durable memories shared across Solmu conversations. Save useful facts only when the user asks or clearly expects them to be remembered. Search before writing to avoid duplicates.",
            json!({
                "action":{"type":"string","enum":["search","read","write","update","delete","list"]},
                "id":{"type":"string","description":"Memory ID for read, update, or delete"},
                "content":{"type":"string","description":"Memory text for write or update"},
                "query":{"type":"string","description":"Words or topic to search for"}
            }),
            &["action"],
        )
    }
    fn execute(
        &self,
        arguments: Value,
        context: Context,
    ) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            let args: Arguments =
                serde_json::from_value(arguments).map_err(|error| error.to_string())?;
            let pool = context.state.pool.clone();
            let value = match args.action.as_str() {
                "search" => json!(
                    memories::relevant(&pool, args.query.as_deref().ok_or("query is required")?)
                        .await
                        .map_err(|error| format!("Cannot search memories: {error:?}"))?
                ),
                "list" => json!(
                    memories::page(&pool, 50, 0)
                        .await
                        .map_err(|error| format!("Cannot list memories: {error:?}"))?
                        .items
                ),
                "read" => json!(
                    memories::get(&pool, args.id.as_deref().ok_or("id is required")?)
                        .await
                        .map_err(|error| format!("Cannot read memory: {error:?}"))?
                ),
                "write" => {
                    let content = args.content.as_deref().ok_or("content is required")?;
                    if content.trim().is_empty() || content.len() > 20_000 {
                        return Err("content must contain 1 to 20000 characters".into());
                    }
                    let memory = memories::write(&pool, content)
                        .await
                        .map_err(|error| format!("Cannot write memory: {error:?}"))?;
                    context.state.memories_changed();
                    json!(memory)
                }
                "update" => {
                    let content = args.content.as_deref().ok_or("content is required")?;
                    if content.trim().is_empty() || content.len() > 20_000 {
                        return Err("content must contain 1 to 20000 characters".into());
                    }
                    let memory = memories::update(
                        &pool,
                        args.id.as_deref().ok_or("id is required")?,
                        content,
                    )
                    .await
                    .map_err(|error| format!("Cannot update memory: {error:?}"))?;
                    context.state.memories_changed();
                    json!(memory)
                }
                "delete" => {
                    memories::delete(&pool, args.id.as_deref().ok_or("id is required")?)
                        .await
                        .map_err(|error| format!("Cannot delete memory: {error:?}"))?;
                    context.state.memories_changed();
                    json!({"deleted":true})
                }
                _ => return Err("Unknown memory action".into()),
            };
            Ok(value)
        })
    }
}
