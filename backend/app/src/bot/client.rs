use crate::{bot::{commands, handlers}, state::AppState};
use anyhow::Result;
use twilight_gateway::{Event, EventTypeFlags, Shard, StreamExt};

pub async fn run(state: AppState, mut shard: Shard) -> Result<()> {
    if let Err(err) = commands::ensure_entry_point_command(state.config.as_ref()).await {
        tracing::warn!(error = %err, "entry point command setup failed");
    }
    register_slash_commands(&state).await?;

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        match item? {
            Event::InteractionCreate(i) => {
                if let Err(err) = handlers::handle_interaction(state.clone(), &i).await {
                    tracing::warn!(error = %err, "interaction handler failed");
                }
            }
            Event::VoiceStateUpdate(v) => handlers::handle_voice_state_update(state.clone(), &v).await,
            Event::VoiceServerUpdate(v) => handlers::handle_voice_server_update(state.clone(), v).await,
            Event::MessageCreate(m) => handlers::handle_message_create(state.clone(), &m).await,
            _ => {}
        }
    }
    Ok(())
}

async fn register_slash_commands(state: &AppState) -> Result<()> {
    let commands = commands::build_commands(state.config.as_ref())?;
    let app_id = state.config.application_id()?;
    let names = commands.iter().map(|command| command.name.clone()).collect::<Vec<_>>();

    if let Some(guild_id) = state.config.discord_guild_id {
        state
            .bot
            .http
            .interaction(app_id)
            .set_guild_commands(guild_id, &commands)
            .await?;
    } else {
        state
            .bot
            .http
            .interaction(app_id)
            .set_global_commands(&commands)
            .await?;
    }

    let _ = state.ready_tx.send(crate::state::BotReady {
        status: "ready".to_string(),
        application_id: app_id.get().to_string(),
        guild_id: state.config.discord_guild_id.map(|id| id.get().to_string()),
        commands: names,
        voice_capabilities: state.voice_engine.voice_capabilities(),
    });
    *state.ready_state.lock().await = Some(crate::state::BotReady {
        status: "ready".to_string(),
        application_id: app_id.get().to_string(),
        guild_id: state.config.discord_guild_id.map(|id| id.get().to_string()),
        commands: commands.iter().map(|command| command.name.clone()).collect(),
        voice_capabilities: state.voice_engine.voice_capabilities(),
    });

    tracing::info!(count = commands.len(), "slash commands registered");
    Ok(())
}
