use axum::{
    Json,
    extract::{
        FromRequest, Request,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::storage::StoreError;

pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetails<'a>,
}

#[derive(Serialize)]
struct ErrorDetails<'a> {
    code: &'a str,
    message: &'a str,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: ErrorDetails {
                    code: self.code,
                    message: &self.message,
                },
            }),
        )
            .into_response()
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(message) => Self::new(StatusCode::NOT_FOUND, "not_found", message),
            StoreError::Conflict(message) => Self::new(StatusCode::CONFLICT, "conflict", message),
            StoreError::Database(error) => {
                eprintln!("Solmu database error: {error}");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Database operation failed",
                )
            }
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(error: JsonRejection) -> Self {
        Self::new(error.status(), "invalid_request", error.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(error: QueryRejection) -> Self {
        Self::new(error.status(), "invalid_request", error.body_text())
    }
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}
