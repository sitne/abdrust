use anyhow::Result;
use abdrust::{bot, config::Config, http, state::AppState, voice_engine::SongbirdVoiceEngine};
use abdrust::dave;
use songbird::{driver::{Channels, DecodeConfig, DecodeMode, SampleRate}, shards::TwilightMap, Songbird};
use std::{collections::HashMap, sync::Arc};
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
    let shard = Shard::new(ShardId::ONE, config.discord_token.clone(), Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES | Intents::GUILD_MESSAGES);
    let bot_user_id = http.current_user().await?.model().await?.id;

    let shard_sender = shard.sender();
    let mut map = HashMap::new();
    map.insert(ShardId::ONE.number(), shard_sender);
    let twilight_map = TwilightMap::new(map);
    let songbird = Songbird::twilight(Arc::new(twilight_map), bot_user_id);
    songbird.set_config(
        songbird::Config::default()
            .decode_mode(DecodeMode::Decode(DecodeConfig::new(Channels::Mono, SampleRate::Hz48000)))
            .use_softclip(true),
    );

    let songbird = Arc::new(songbird);
    let voice_engine = Arc::new(SongbirdVoiceEngine::new(songbird.clone()));
    tracing::info!(dave_protocol_version = dave::MAX_DAVE_PROTOCOL_VERSION, "DAVE support available");
    let state = AppState::new(config.clone(), Arc::new(http), voice_engine);

    let bot_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = bot::client::run(bot_state, shard).await {
            tracing::error!(error = %err, "bot task failed");
        }
    });

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
