use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, State},
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{Stream, StreamExt};
use genai::chat::ChatStreamEvent;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::storage::messages;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateResponse {
    message_id: String,
}

fn event(kind: &str, data: impl Serialize) -> Event {
    Event::default()
        .event(kind)
        .json_data(data)
        .expect("SSE data must be JSON serializable")
}

pub async fn create(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    ApiJson(input): ApiJson<CreateResponse>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let permit = state.response(&thread_id)?;
    let token = permit.token.clone();
    let history = messages::history_for_reply(&state.pool, &thread_id, &input.message_id).await?;
    let profile = crate::storage::profile::get(&state.pool).await?;
    let thread = crate::storage::threads::get(&state.pool, &thread_id).await?;
    let mut reply = tokio::select! {
        result = state.llm.stream(&history, &profile.system_prompt, thread.model.as_deref().or(profile.model.as_deref())) => result?,
        _ = token.cancelled() => return Err(stopped()),
    };
    let first = tokio::select! {
      _ = token.cancelled() => return Err(stopped()),
      result = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(event) = reply.stream.next().await {
            match event {
                Ok(event @ ChatStreamEvent::Chunk(_)) => return Some(event),
                Err(_) | Ok(ChatStreamEvent::End(_)) => return None,
                _ => {},
            }
        }
        None
      }) => result.ok().flatten(),
    }
    .ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "provider_error",
            "LLM request failed; check provider credentials, model, and endpoint",
        )
    })?;
    let stream = async_stream::stream! {
        let _permit = permit;
        yield Ok(event("start", json!({"message_id": input.message_id})));
        let mut content = String::new();
        let mut finished = false;
        let mut pending = Some(first);
        loop {
            if token.is_cancelled() { yield Ok(event("stopped", json!({"message_id": input.message_id}))); return; }
            let next = if let Some(first) = pending.take() { Ok(Some(Ok(first))) } else {
                tokio::select! {
                    _ = token.cancelled() => { yield Ok(event("stopped", json!({"message_id": input.message_id}))); return; },
                    result = tokio::time::timeout(Duration::from_secs(60), reply.stream.next()) => result,
                }
            };
            match next {
                Ok(Some(Ok(ChatStreamEvent::Chunk(chunk)))) => {
                    content.push_str(&chunk.content);
                    yield Ok(event("delta", json!({"text": chunk.content})));
                }
                Ok(Some(Ok(ChatStreamEvent::End(_)))) => { finished = true; break; }
                Ok(Some(Ok(_))) => {},
                Ok(None) => break,
                Ok(Some(Err(_))) | Err(_) => {
                    yield Ok(event("error", json!({"error": {"code": "provider_error", "message": "The provider stream failed or timed out"}})));
                    return;
                }
            }
        }
        if !finished || content.trim().is_empty() {
            yield Ok(event("error", json!({"error": {"code": "incomplete_response", "message": "The provider did not complete a text response"}})));
            return;
        }
        match messages::create(&state.pool, &thread_id, "assistant", &content, Some(&input.message_id)).await {
            Ok(message) => { state.changed(&thread_id); yield Ok(event("done", message)); },
            Err(error) => {
                let error = ApiError::from(error);
                yield Ok(event("error", json!({"error": {"code": error.code, "message": error.message}})));
            }
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
    Ok(axum::http::StatusCode::NO_CONTENT)
}
