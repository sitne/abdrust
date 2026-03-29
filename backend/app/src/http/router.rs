use crate::{
    http::{activity, api, ws},
    state::AppState,
};
use axum::{
    http::HeaderValue,
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};

pub fn router(state: AppState) -> Router {
    let cors = if state.config.cors_allow_all {
        CorsLayer::new()
            .allow_origin(AllowOrigin::any())
            .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION])
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
            .allow_credentials(false)
    } else {
        let mut origins = state.config.cors_allowed_origins.clone();
        if origins.is_empty() {
            origins.push(HeaderValue::from_static("https://discord.com"));
            origins.push(HeaderValue::from_static("https://canary.discord.com"));
            origins.push(HeaderValue::from_static("https://ptb.discord.com"));
        }
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION])
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
            .allow_credentials(false)
    };

    Router::new()
        .route("/api/token", post(activity::exchange_token))
        .route("/api/guild/{guild_id}", get(api::guild))
        .route("/api/guild/{guild_id}/voice", get(api::voice))
        .route("/api/private/guild/{guild_id}/voice", get(api::voice_private))
        .route("/ws", get(ws::ws))
        .route("/api/health", get(|| async { axum::Json(serde_json::json!({"ok": true})) }))
        .fallback_service(
            ServeDir::new(state.config.frontend_dist.clone())
                .append_index_html_on_directories(true),
        )
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

pub fn slash_command_router(state: &AppState) -> Router {
    Router::new().with_state(state.clone())
}
