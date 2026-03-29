use crate::{config::Config, error::AppError};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthSession {
    pub session_id: String,
    pub access_token: String,
    pub created_at_ms: u64,
    pub guild_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthContext {
    pub session_id: String,
    pub guild_id: Option<String>,
}

pub fn session_header_name() -> &'static str {
    "x-abdrust-session-id"
}

pub fn auth_mode_enabled(config: &Config) -> bool {
    matches!(config.activity_mode, crate::config::ActivityMode::Discord)
}

pub async fn validate_session_id(state: &crate::state::AppState, headers: &HeaderMap) -> Result<AuthContext, AppError> {
    let session_id = headers
        .get(session_header_name())
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if !auth_mode_enabled(&state.config) {
        if let Some(session_id) = session_id {
            if let Some(session) = state.auth_session(&session_id).await {
                return Ok(AuthContext {
                    session_id: session.session_id,
                    guild_id: None,
                });
            }
        }

        return Ok(AuthContext {
            session_id: "local".to_string(),
            guild_id: None,
        });
    }

    let session_id = session_id.ok_or_else(|| AppError::Unauthorized("missing session id header".to_string()))?;

    let Some(session) = state.auth_session(&session_id).await else {
        return Err(AppError::Unauthorized("unknown session id".to_string()));
    };

    Ok(AuthContext {
        session_id: session.session_id,
        guild_id: None,
    })
}

pub async fn authorize_guild_access(state: &crate::state::AppState, headers: &HeaderMap, guild_id: &str) -> Result<AuthContext, AppError> {
    if !auth_mode_enabled(&state.config) {
        return Ok(AuthContext {
            session_id: "local".to_string(),
            guild_id: Some(guild_id.to_string()),
        });
    }

    let auth = validate_session_id(state, headers).await?;
    let Some(session) = state.auth_session(&auth.session_id).await else {
        return Err(AppError::Unauthorized("unknown session id".to_string()));
    };

    if !session.guild_ids.iter().any(|allowed| allowed == guild_id) {
        return Err(AppError::Forbidden("session is not authorized for this guild".to_string()));
    }

    Ok(AuthContext {
        session_id: auth.session_id,
        guild_id: Some(guild_id.to_string()),
    })
}

pub async fn authorized_session(state: &crate::state::AppState, headers: &HeaderMap) -> Result<AuthContext, AppError> {
    validate_session_id(state, headers).await
}

pub async fn authorize_session_guild(state: &crate::state::AppState, session_id: &str, guild_id: &str) -> Result<AuthContext, AppError> {
    let Some(session) = state.auth_session(session_id).await else {
        return Err(AppError::Unauthorized("unknown session id".to_string()));
    };

    if auth_mode_enabled(&state.config) && !session.guild_ids.iter().any(|allowed| allowed == guild_id) {
        return Err(AppError::Forbidden("session is not authorized for this guild".to_string()));
    }

    Ok(AuthContext {
        session_id: session.session_id,
        guild_id: Some(guild_id.to_string()),
    })
}
