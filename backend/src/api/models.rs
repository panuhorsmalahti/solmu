use super::state::AppState;
use crate::llm::ModelCatalog;
use axum::{Json, extract::State};

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<ModelCatalog>, super::error::ApiError> {
    let profile = crate::storage::profile::get(&state.pool).await?;
    let mut catalog = state.llm.models();
    catalog.default_model = state.llm.backend_default_model().await;
    if let Some(model) = profile.model {
        catalog.default_model = Some(model);
    }
    Ok(Json(catalog))
}
