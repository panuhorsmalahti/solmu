use super::{
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::storage::profile::{self, Profile};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ProfileView {
    #[serde(flatten)]
    profile: Profile,
    backend_default_model: Option<String>,
}

async fn view(state: &AppState, profile: Profile) -> Json<ProfileView> {
    Json(ProfileView {
        profile,
        backend_default_model: state.llm.backend_default_model().await,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditProfile {
    system_prompt: String,
    #[serde(default, deserialize_with = "super::threads::present_model")]
    model: Option<Option<String>>,
}

pub async fn get(State(state): State<AppState>) -> Result<Json<ProfileView>, ApiError> {
    Ok(view(&state, profile::get(&state.pool).await?).await)
}

pub async fn save(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<EditProfile>,
) -> Result<Json<ProfileView>, ApiError> {
    if input.system_prompt.trim().is_empty() || input.system_prompt.len() > 64_000 {
        return Err(ApiError::invalid(
            "System prompt must contain text and be at most 64000 bytes",
        ));
    }
    if let Some(Some(model)) = &input.model {
        state.llm.validate_model(model)?;
    }
    let model = input.model.as_ref().map(|model| model.as_deref());
    let profile = profile::save(&state.pool, &input.system_prompt, model).await?;
    state.profile_changed();
    Ok(view(&state, profile).await)
}
