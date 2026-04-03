use crate::{bot::{commands, handlers}, presence, state::AppState};
use anyhow::Result;
use twilight_gateway::{Event, EventTypeFlags, Shard, StreamExt};

pub async fn run(state: AppState, mut shard: Shard) -> Result<()> {
    if let Err(err) = commands::ensure_entry_point_command(state.config.as_ref()).await {
        tracing::warn!(error = %err, "entry point command setup failed");
    }
    register_slash_commands(&state).await?;

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let event = item?;
        tracing::debug!(event_type = ?event.kind(), "received gateway event");
        match event {
            Event::InteractionCreate(i) => {
                tracing::debug!(interaction_id = %i.id, interaction_type = ?i.kind, "received interaction");
                if let Err(err) = handlers::handle_interaction(state.clone(), &i).await {
                    tracing::error!(error = %err, "interaction handler failed");
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
    let ready = crate::state::BotReady {
        status: "ready".to_string(),
        application_id: app_id.get().to_string(),
        guild_id: state.config.discord_guild_id.map(|id| id.get().to_string()),
        commands: commands.iter().map(|command| command.name.clone()).collect(),
        voice_capabilities: state.voice_engine.voice_capabilities(),
    };
    presence::send_ready(&state.bot.gateway, &ready);
    presence::send_status(&state.bot.gateway, "ready");
    *state.ready_state.lock().await = Some(ready);

    tracing::info!(count = commands.len(), "slash commands registered");
    Ok(())
}
