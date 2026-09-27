use std::time::Duration;

use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::{Value, json};
use solmu_e2e::support::Backend;
use sqlx::Connection;

async fn complete_reply(backend: &Backend, id: &str, message: &Value) -> Vec<String> {
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut events = response.bytes_stream().eventsource();
    let mut names = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(10), events.next())
        .await
        .unwrap()
    {
        names.push(event.unwrap().event);
    }
    names
}

mod models;
mod profile;
mod profile_model;
mod system_prompt_history;
mod threads;
mod workspaces;

mod responses;

mod events;

mod configuration;
mod skills;
mod tools;
mod web;
