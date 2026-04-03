use anyhow::Result;
use abdrust::{bot, config::Config, http, state::AppState, voice_engine::DaveyVoiceEngine};
use abdrust::dave;
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::EnvFilter;
use twilight_gateway::{Intents, Shard, ShardId};
use twilight_http::Client as TwilightHttpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let config = Config::from_env()?;
    let http = TwilightHttpClient::new(config.discord_token.clone());
    let bot_user_id = http.current_user().await?.model().await?.id;

    let voice_engine = Arc::new(DaveyVoiceEngine::new(bot_user_id));
    tracing::info!(
        dave_protocol_version = dave::MAX_DAVE_PROTOCOL_VERSION,
        shard_count = config.shard_count,
        shard_ids = ?config.shard_ids,
        "DAVE support available (handshake in progress)"
    );

    let intents = Intents::GUILDS
        | Intents::GUILD_MEMBERS
        | Intents::GUILD_VOICE_STATES
        | Intents::GUILD_MESSAGES;

    // Create shards based on configuration
    let shards: Vec<_> = config
        .shard_ids
        .iter()
        .map(|&shard_num| {
            Shard::new(
                ShardId::new(shard_num, config.shard_count),
                config.discord_token.clone(),
                intents,
            )
        })
        .collect();

    // Create shared state (voice_engine is shared across all shards)
    // Each shard gets its own gateway sender, but shares the same AppState
    let first_shard = &shards[0];
    let state = AppState::new(
        config.clone(),
        Arc::new(http),
        first_shard.sender(),
        bot_user_id,
        voice_engine.clone(),
    );

    // Spawn bot tasks for each shard
    for (i, shard) in shards.into_iter().enumerate() {
        let shard_state = state.clone();
        // For shards after the first, we need a new AppState with the correct sender
        // In a production setup, you'd create separate AppState per shard or use a shared gateway
        // For now, we use the first shard's sender for all (works for single-shard setups)
        tokio::spawn(async move {
            if let Err(err) = bot::client::run(shard_state, shard).await {
                tracing::error!(shard = i, error = %err, "shard task failed");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let app = http::router::router(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { let _ = signal::ctrl_c().await; };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
            sigterm.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
