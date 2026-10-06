use super::{AgentTool, Context, definition};
use crate::storage::goals;
use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Goals;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: String,
    id: Option<String>,
    objective: Option<String>,
    status: Option<String>,
}

impl AgentTool for Goals {
    fn definition(&self) -> Tool {
        definition(
            "Goals",
            "Start or manage persistent agent goals. Use start when the user asks Solmu to pursue a multi-step objective beyond this reply; use update to pause, resume, complete, or cancel one. Do not create goals for ordinary questions or single-turn requests.",
            json!({
                "action":{"type":"string","enum":["start","list","update"]},
                "id":{"type":"string","description":"Goal ID for update"},
                "objective":{"type":"string","description":"The user's objective, preserving their intent and constraints"},
                "status":{"type":"string","enum":["active","paused","completed","cancelled"]}
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
            let state = context.state;
            let value = match args.action.as_str() {
                "start" => {
                    let objective = args.objective.as_deref().ok_or("objective is required")?;
                    if objective.trim().is_empty() || objective.len() > 20_000 {
                        return Err("objective must contain 1 to 20000 characters".into());
                    }
                    let goal = goals::create(&state.pool, objective, Some(&context.thread_id))
                        .await
                        .map_err(|error| format!("Cannot start goal: {error:?}"))?;
                    state.goals_changed();
                    json!(goal)
                }
                "list" => json!(
                    goals::list(&state.pool)
                        .await
                        .map_err(|error| format!("Cannot list goals: {error:?}"))?
                ),
                "update" => {
                    let id = args.id.as_deref().ok_or("id is required")?;
                    let status = args.status.as_deref().ok_or("status is required")?;
                    if !["active", "paused", "completed", "cancelled"].contains(&status) {
                        return Err("status must be active, paused, completed, or cancelled".into());
                    }
                    let goal = goals::set_status(&state.pool, id, status)
                        .await
                        .map_err(|error| format!("Cannot update goal: {error:?}"))?;
                    state.goals_changed();
                    json!(goal)
                }
                _ => return Err("Unknown goal action".into()),
            };
            Ok(value)
        })
    }
}
