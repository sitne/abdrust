use crate::{auth, error::AppError, state::AppState};
use axum::{extract::{Path, State}, http::HeaderMap, Json};
use serde_json::json;
use twilight_model::id::{marker::GuildMarker, Id};

pub async fn guild(State(state): State<AppState>, headers: HeaderMap, Path(guild_id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    let _auth = auth::authorize_guild_access(&state, &headers, &guild_id).await?;
    let guild_id = guild_id.parse::<u64>().map_err(|e| AppError::BadRequest(e.to_string()))?;
    let request = state.bot.http.guild(Id::<GuildMarker>::new(guild_id));
    let response = request.await.map_err(|e| AppError::Message(e.to_string()))?;
    let guild = response.model().await.map_err(|e| AppError::Message(e.to_string()))?;
    Ok(Json(json!({ "id": guild.id.get().to_string(), "name": guild.name, "member_count": guild.approximate_member_count })))
}

pub async fn voice(State(state): State<AppState>, headers: HeaderMap, Path(guild_id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    let _auth = auth::authorize_guild_access(&state, &headers, &guild_id).await?;
    let guild_id_num = guild_id.parse::<u64>().map_err(|e| AppError::BadRequest(e.to_string()))?;
    let guild_key = Id::new(guild_id_num);
    let diagnostics = state.voice_diagnostics(guild_key).await;
    Ok(Json(json!(diagnostics)))
}

pub async fn voice_private(State(state): State<AppState>, headers: HeaderMap, Path(guild_id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    let _auth = auth::authorize_guild_access(&state, &headers, &guild_id).await?;
    let guild_id_num = guild_id.parse::<u64>().map_err(|e| AppError::BadRequest(e.to_string()))?;
    let guild_key = Id::new(guild_id_num);
    let diagnostics = state.voice_diagnostics(guild_key).await;
    Ok(Json(json!({
        "guild_id": diagnostics.guild_id,
        "voice": diagnostics.voice,
        "join_state": diagnostics.join_state,
        "voice_capabilities": diagnostics.voice_capabilities,
        "signal_trace": diagnostics.signal_trace,
        "receive_trace": diagnostics.receive_trace,
    })))
}
