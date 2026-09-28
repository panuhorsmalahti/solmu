use super::{
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::{
    storage::{messages, tools},
    tools::{Context, ResultData},
};
use axum::{
    extract::{Path, State},
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{Stream, StreamExt};
use genai::chat::{ChatMessage, ChatStreamEvent, ToolResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{convert::Infallible, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateResponse {
    message_id: String,
}
fn event(kind: &str, data: impl Serialize) -> Event {
    Event::default()
        .event(kind)
        .json_data(data)
        .expect("SSE JSON")
}
fn failed(message: &str) -> Event {
    event(
        "error",
        json!({"error":{"code":"provider_error","message":message}}),
    )
}

struct Recovery {
    state: AppState,
    thread_id: String,
    ids: Vec<String>,
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let state = self.state.clone();
            let thread_id = self.thread_id.clone();
            let ids = std::mem::take(&mut self.ids);
            runtime.spawn(async move {
                if tools::interrupt_runs(&state.pool, &ids).await.is_ok() {
                    state.changed(&thread_id);
                }
            });
        }
    }
}

pub async fn create(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    ApiJson(input): ApiJson<CreateResponse>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let permit = state.response(&thread_id)?;
    let token = permit.token.clone();
    let messages = messages::history_for_reply(&state.pool, &thread_id, &input.message_id).await?;
    tools::recover(&state.pool, Some(&thread_id)).await?;
    let mut history = tools::history(&state.pool, &thread_id, &messages).await?;
    let profile = crate::storage::profile::get(&state.pool).await?;
    let thread = crate::storage::threads::get(&state.pool, &thread_id).await?;
    let workspace = crate::workspace::resolve(thread.workspace.as_deref())?;
    if thread.workspace.is_none() {
        sqlx::query("UPDATE threads SET workspace=? WHERE id=?")
            .bind(&workspace)
            .bind(&thread_id)
            .execute(&state.pool)
            .await
            .map_err(crate::storage::StoreError::from)?;
    }
    let model = thread.model.clone().or(profile.model.clone());
    let skills = state
        .skills
        .load(&thread_id, workspace.clone().into())
        .await;
    history.insert(0, ChatMessage::user(skills.context()));
    let mcp = tokio::select! {
        snapshot = state.mcp.load(&thread_id, workspace.clone().into()) => snapshot,
        _ = token.cancelled() => return Err(stopped()),
    };
    let mut registry = state.tools.clone();
    for tool in mcp.tools {
        registry.register(tool);
    }
    let mut reply = tokio::select! {
        result=state.llm.stream(&history,&profile.system_prompt,model.as_deref(),registry.definitions())=>result?,
        _=token.cancelled()=>return Err(stopped()),
    };
    // Fail before sending HTTP 200 when the provider cannot begin a response.
    // Tool-only turns are valid, including an End event carrying tool calls.
    let first=tokio::select! {
        _=token.cancelled()=>return Err(stopped()),
        result=tokio::time::timeout(Duration::from_secs(30),async {
            while let Some(event)=reply.stream.next().await {
                match event {
                    Ok(event @ (ChatStreamEvent::Chunk(_)|ChatStreamEvent::ToolCallChunk(_)|ChatStreamEvent::End(_)))=>return Some(event),
                    Err(_)=>return None,
                    _=>{},
                }
            }
            None
        })=>result.ok().flatten(),
    }.ok_or_else(||ApiError::new(axum::http::StatusCode::BAD_GATEWAY,"provider_error","LLM request failed; check provider credentials, model, and endpoint"))?;
    let stream = async_stream::stream! {
        let _permit=permit;
        let mut recovery=Recovery{state:state.clone(),thread_id:thread_id.clone(),ids:Vec::new()};
        yield Ok(event("start",json!({"message_id":input.message_id})));
        let context=Context{workspace:workspace.into(),skill_roots:skills.roots,cancellation:token.clone()};
        let mut pending=Some(first);
        let mut total_calls=0;
        for round in 0..16 {
            let mut content=String::new();
            let end=loop {
                if token.is_cancelled() {yield Ok(event("stopped",json!({"message_id":input.message_id})));return;}
                let next=if let Some(first)=pending.take() {Ok(Some(Ok(first)))} else {
                    tokio::select! {
                        _=token.cancelled()=>{yield Ok(event("stopped",json!({"message_id":input.message_id})));return;},
                        result=tokio::time::timeout(Duration::from_secs(60),reply.stream.next())=>result,
                    }
                };
                match next {
                    Ok(Some(Ok(ChatStreamEvent::Chunk(chunk))))=>{
                        if content.len()+chunk.content.len()>1_000_000 {yield Ok(failed("The response exceeded the size limit"));return;}
                        content.push_str(&chunk.content);yield Ok(event("delta",json!({"text":chunk.content})));
                    },
                    Ok(Some(Ok(ChatStreamEvent::End(end))))=>break end,
                    Ok(Some(Ok(_)))=>{},
                    _=>{yield Ok(failed("The provider stream failed or ended before completion"));return;},
                }
            };
            let calls=end.captured_tool_calls().unwrap_or_default().into_iter().cloned().collect::<Vec<_>>();
            if calls.is_empty() {
                if content.trim().is_empty() {yield Ok(failed("The provider did not complete a text response"));return;}
                match messages::create(&state.pool,&thread_id,"assistant",&content,Some(&input.message_id)).await {
                    Ok(message)=>{state.changed(&thread_id);yield Ok(event("done",message));},
                    Err(_)=>yield Ok(failed("Cannot save the completed response")),
                }
                return;
            }
            total_calls+=calls.len();
            let mut ids=std::collections::HashSet::new();
            if total_calls>64 || calls.iter().any(|call|call.call_id.is_empty()||call.call_id.len()>200||!ids.insert(call.call_id.clone())||call.fn_name.len()>200||call.fn_arguments.to_string().len()>32_000) {
                yield Ok(failed("The provider returned too many or invalid tool calls"));return;
            }
            let assistant=ChatMessage::assistant(end.captured_content.unwrap_or_default()).with_reasoning_content(end.captured_reasoning_content);
            let runs=match tools::create_turn(&state.pool,&thread_id,&input.message_id,&assistant).await {
                Ok(runs)=>runs,
                Err(_)=>{yield Ok(failed("Cannot save the tool calls"));return;},
            };
            history.push(assistant);
            recovery.ids.extend(runs.iter().map(|run|run.id.clone()));
            yield Ok(event("reset",json!({})));
            for run in runs {
                let result=if token.is_cancelled() {ResultData::error("Tool cancelled before execution")} else {
                    let started=match tools::start(&state.pool,&run.id).await {
                        Ok(started)=>started,
                        Err(_)=>{yield Ok(failed("Cannot record tool start"));return;},
                    };
                    state.changed(&thread_id);
                    yield Ok(event("tool_start",started));
                    registry.execute(&run.name,run.arguments.clone(),context.clone()).await
                };
                let status=if result.output["error"].as_str().is_some_and(|error|error.starts_with("Tool cancelled")) {"cancelled"} else if result.success {"completed"} else {"failed"};
                match tools::finish(&state.pool,&run.id,status,&result).await {
                    Ok(finished)=>{recovery.ids.retain(|id|id!=&run.id);state.changed(&thread_id);yield Ok(event("tool_result",finished));},
                    Err(_)=>{yield Ok(failed("Cannot save tool result"));return;},
                }
                history.push(ToolResponse::new(run.call_id,serde_json::to_string(&result).expect("tool JSON")).into());
            }
            if token.is_cancelled() {yield Ok(event("stopped",json!({"message_id":input.message_id})));return;}
            if round==15 {yield Ok(failed("The response reached the 16-round tool limit"));return;}
            reply=tokio::select! {
                _=token.cancelled()=>{yield Ok(event("stopped",json!({"message_id":input.message_id})));return;},
                result=state.llm.stream(&history,&profile.system_prompt,model.as_deref(),registry.definitions())=>match result {
                    Ok(reply)=>reply,
                    Err(_)=>{yield Ok(failed("The provider failed after the tool results"));return;},
                },
            };
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
fn stopped() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "response_stopped",
        "Response stopped",
    )
}
pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    crate::storage::threads::get(&state.pool, &id).await?;
    state.stop(&id);
    state.wait_stopped(&id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
