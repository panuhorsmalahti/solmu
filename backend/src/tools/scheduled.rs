use super::{AgentTool, Context, definition};
use crate::storage::scheduled_tasks;
use chrono::Utc;
use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Tasks;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: String,
    id: Option<String>,
    name: Option<String>,
    prompt: Option<String>,
    schedule_kind: Option<String>,
    schedule: Option<String>,
    enabled: Option<bool>,
}

fn required<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .ok_or_else(|| format!("{field} is required"))
}

fn validate(name: &str, prompt: &str, kind: &str, schedule: &str) -> Result<String, String> {
    if name.trim().is_empty()
        || name.len() > 120
        || prompt.trim().is_empty()
        || prompt.len() > 20_000
        || schedule.len() > 120
    {
        return Err("Invalid task name, prompt, or schedule length".into());
    }
    scheduled_tasks::next_run(kind, schedule, Utc::now())
}

impl AgentTool for Tasks {
    fn definition(&self) -> Tool {
        definition(
            "Tasks",
            "Manage scheduled agent tasks. New tasks use the current workspace. Create only when the user explicitly requests a schedule. Cron uses five fields in UTC; one-shot uses a future RFC 3339 timestamp. Each task runs in its own saved conversation.",
            json!({
                "action":{"type":"string","enum":["create","list","update","pause","resume","run","remove"]},
                "id":{"type":"string","description":"Task ID for update, pause, resume, run, or remove"},
                "name":{"type":"string"},
                "prompt":{"type":"string"},
                "schedule_kind":{"type":"string","enum":["once","cron"]},
                "schedule":{"type":"string","description":"RFC 3339 time or five-field UTC cron expression"},
                "enabled":{"type":"boolean"}
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
            let pool = &state.pool;
            let changed = match args.action.as_str() {
                "create" => {
                    let name = required(&args.name, "name")?;
                    let prompt = required(&args.prompt, "prompt")?;
                    let kind = required(&args.schedule_kind, "schedule_kind")?;
                    let schedule = required(&args.schedule, "schedule")?;
                    let next = validate(name, prompt, kind, schedule)?;
                    let workspace = context.workspace.to_string_lossy();
                    let task = scheduled_tasks::create(
                        pool,
                        name.trim(),
                        prompt.trim(),
                        kind,
                        schedule.trim(),
                        &workspace,
                        &next,
                    )
                    .await
                    .map_err(|error| format!("Cannot create task: {error:?}"))?;
                    state.changed(&task.thread_id);
                    json!(task)
                }
                "list" => json!(
                    scheduled_tasks::list(pool)
                        .await
                        .map_err(|error| format!("Cannot list tasks: {error:?}"))?
                ),
                "update" | "pause" | "resume" => {
                    let id = required(&args.id, "id")?;
                    let old = scheduled_tasks::get(pool, id)
                        .await
                        .map_err(|error| format!("Cannot get task: {error:?}"))?;
                    let name = args.name.as_deref().unwrap_or(&old.name);
                    let prompt = args.prompt.as_deref().unwrap_or(&old.prompt);
                    let kind = args.schedule_kind.as_deref().unwrap_or(&old.schedule_kind);
                    let schedule = args.schedule.as_deref().unwrap_or(&old.schedule);
                    if name.trim().is_empty()
                        || name.len() > 120
                        || prompt.trim().is_empty()
                        || prompt.len() > 20_000
                        || schedule.len() > 120
                    {
                        return Err("Invalid task name, prompt, or schedule length".into());
                    }
                    let enabled = match args.action.as_str() {
                        "pause" => false,
                        "resume" => true,
                        _ => args.enabled.unwrap_or(old.enabled),
                    };
                    let next = if enabled {
                        Some(validate(name, prompt, kind, schedule)?)
                    } else {
                        if kind == "cron" {
                            scheduled_tasks::next_run(kind, schedule, Utc::now())?;
                        } else if kind == "once" {
                            chrono::DateTime::parse_from_rfc3339(schedule).map_err(|_| {
                                "One-shot time must be an RFC 3339 timestamp with a timezone"
                                    .to_owned()
                            })?;
                        } else {
                            return Err("Schedule kind must be once or cron".into());
                        }
                        old.next_run_at.clone()
                    };
                    let task = scheduled_tasks::update(
                        pool,
                        id,
                        scheduled_tasks::TaskChanges {
                            name: name.trim(),
                            prompt: prompt.trim(),
                            kind,
                            schedule: schedule.trim(),
                            enabled,
                            next: next.as_deref(),
                        },
                    )
                    .await
                    .map_err(|error| format!("Cannot update task: {error:?}"))?;
                    state.changed(&task.thread_id);
                    json!(task)
                }
                "run" => {
                    let id = required(&args.id, "id")?;
                    let (task, run) = scheduled_tasks::run_now(pool, id)
                        .await
                        .map_err(|error| format!("Cannot run task: {error:?}"))?;
                    tokio::spawn(crate::scheduler::execute(state.clone(), task, run.clone()));
                    json!(run)
                }
                "remove" => {
                    let id = required(&args.id, "id")?;
                    scheduled_tasks::delete(pool, id)
                        .await
                        .map_err(|error| format!("Cannot remove task: {error:?}"))?;
                    json!({"removed":id})
                }
                _ => return Err("Unknown task action".into()),
            };
            if args.action != "list" {
                state.tasks_changed();
            }
            Ok(changed)
        })
    }
}
