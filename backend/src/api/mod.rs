pub mod error;
mod events;
mod mcp;
mod messages;
mod models;
mod plugins;
mod profile;
pub(crate) mod responses;
mod scheduled_tasks;
mod skills;
pub mod state;
mod threads;
mod tools;
pub(crate) mod webhooks;

use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use state::AppState;

use error::ApiError;

#[derive(Default, Deserialize)]
pub struct Pagination {
    limit: Option<u32>,
    offset: Option<u32>,
}

impl Pagination {
    fn values(self) -> Result<(u32, u32), ApiError> {
        let limit = self.limit.unwrap_or(50);
        if !(1..=100).contains(&limit) {
            return Err(ApiError::invalid("limit must be between 1 and 100"));
        }
        Ok((limit, self.offset.unwrap_or(0)))
    }
}

#[derive(Serialize)]
pub struct ListResponse<T> {
    items: Vec<T>,
    limit: u32,
    offset: u32,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/tools", get(tools::definitions))
        .route("/api/v1/audit", get(tools::audit))
        .route(
            "/api/v1/tasks",
            get(scheduled_tasks::list).post(scheduled_tasks::create),
        )
        .route(
            "/api/v1/tasks/{id}",
            get(scheduled_tasks::get)
                .patch(scheduled_tasks::update)
                .delete(scheduled_tasks::delete),
        )
        .route("/api/v1/tasks/{id}/runs", get(scheduled_tasks::runs))
        .route("/api/v1/tasks/{id}/run", post(scheduled_tasks::run_now))
        .route("/api/v1/threads/{thread_id}/skills", get(skills::list))
        .route("/api/v1/threads/{thread_id}/mcp", get(mcp::list))
        .route("/api/v1/threads/{thread_id}/plugins", get(plugins::list))
        .route("/api/v1/threads/{thread_id}/tools", get(tools::list))
        .route("/api/v1/profile", get(profile::get).put(profile::save))
        .route(
            "/api/v1/webhooks",
            get(webhooks::list).post(webhooks::create),
        )
        .route(
            "/api/v1/webhooks/{id}",
            axum::routing::patch(webhooks::update).delete(webhooks::delete),
        )
        .route("/api/v1/models", get(models::list))
        .route("/api/v1/events", get(events::connect))
        .route("/api/v1/threads/{thread_id}/stop", post(responses::stop))
        .route("/api/v1/threads", get(threads::list).post(threads::create))
        .route(
            "/api/v1/threads/{thread_id}",
            get(threads::get)
                .patch(threads::update)
                .delete(threads::delete),
        )
        .route(
            "/api/v1/threads/{thread_id}/messages",
            get(messages::list).post(messages::create),
        )
        .route(
            "/api/v1/threads/{thread_id}/responses",
            post(responses::create),
        )
        .fallback(|| async {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "Endpoint not found")
        })
        .method_not_allowed_fallback(|| async {
            ApiError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "Method not allowed",
            )
        })
        .with_state(state)
}
