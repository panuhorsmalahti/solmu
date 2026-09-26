pub mod error;
mod messages;
mod responses;
pub mod state;
mod threads;

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
