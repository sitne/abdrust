use crate::{auth::AuthSession, error::AppError, state::AppState};
use axum::{extract::State, Json};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct TokenRequest {
    pub code: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub session_id: String,
}

pub async fn exchange_token(State(state): State<AppState>, Json(req): Json<TokenRequest>) -> Result<Json<TokenResponse>, AppError> {
    let client = Client::new();
    let params = [
        ("client_id", state.config.discord_client_id.as_str()),
        ("client_secret", state.config.discord_client_secret.as_str()),
        ("grant_type", "authorization_code"),
        ("code", req.code.as_str()),
        ("redirect_uri", state.config.discord_redirect_uri.as_str()),
    ];
    let resp = client
        .post("https://discord.com/api/oauth2/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!("token exchange failed: {} {}", status, body)));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    let access_token = json.get("access_token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if access_token.is_empty() {
        return Err(AppError::BadRequest("token exchange returned no access_token".to_string()));
    }
    let session_id = Uuid::new_v4().to_string();
    let created_at_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| AppError::BadRequest(e.to_string()))?.as_millis() as u64;
    state.store_auth_session(AuthSession { session_id: session_id.clone(), access_token: access_token.clone(), created_at_ms, guild_ids: state.config.discord_guild_id.map(|id| vec![id.get().to_string()]).unwrap_or_default() }).await;
    Ok(Json(TokenResponse { access_token, session_id }))
}
