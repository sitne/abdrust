use crate::{bot::commands::DEBUG_COMMAND_NAME, dave, state::{AppEvent, AppState, VoiceJoinState, VoiceSession}, voice};
use anyhow::Result;
use twilight_model::{
    application::interaction::{InteractionData, InteractionType},
    gateway::payload::incoming::{InteractionCreate, MessageCreate, VoiceStateUpdate, VoiceServerUpdate},
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};

pub async fn handle_interaction(state: AppState, event: &InteractionCreate) -> Result<()> {
    if event.kind != InteractionType::ApplicationCommand {
        return Ok(());
    }

    let Some(InteractionData::ApplicationCommand(command)) = &event.data else { return Ok(()); };
    match command.name.as_str() {
        DEBUG_COMMAND_NAME => {
            let response = InteractionResponse {
                kind: InteractionResponseType::ChannelMessageWithSource,
                data: Some(InteractionResponseData {
                    content: Some(format!("abdrust debug command received: {}", command.name)),
                    ..InteractionResponseData::default()
                }),
            };

            let _ = state.event_tx.send(AppEvent::Custom {
                name: "debug_command".to_string(),
                payload: serde_json::json!({
                    "command": command.name,
                    "guild_id": command.guild_id.map(|id| id.get().to_string()),
                }),
            });

            let _ = state.ready_tx.send(crate::state::BotReady {
                status: "command_invoked".to_string(),
                application_id: event.application_id.get().to_string(),
                guild_id: command.guild_id.map(|id| id.get().to_string()),
                commands: vec![command.name.clone()],
                voice_capabilities: state.voice_engine.voice_capabilities(),
            });
            *state.ready_state.lock().await = Some(crate::state::BotReady {
                status: "command_invoked".to_string(),
                application_id: event.application_id.get().to_string(),
                guild_id: command.guild_id.map(|id| id.get().to_string()),
                commands: vec![command.name.clone()],
                voice_capabilities: state.voice_engine.voice_capabilities(),
            });

            state
                .bot
                .http
                .interaction(event.application_id)
                .create_response(event.id, &event.token, &response)
                .await?;
        }
        "voice" => handle_voice_command(state, event, command).await?,
        "voice-diag" => handle_voice_diag_command(state, event, command).await?,
        _ => {}
    }

    Ok(())
}

async fn handle_voice_command(state: AppState, event: &InteractionCreate, command: &twilight_model::application::interaction::application_command::CommandData) -> Result<()> {
    let Some(guild_id) = command.guild_id else { return Ok(()); };
    let sub_name = command.options.first().map(|opt| opt.name.as_str()).unwrap_or("");
    let response_text = match sub_name {
        "join" => {
            let user_id = event.author_id().ok_or_else(|| anyhow::anyhow!("missing user id"))?;
            let channel_id = {
                state.user_voice_channel(guild_id, user_id).await
            };
            if let Some(channel_id) = channel_id {
                let deferred = InteractionResponse {
                    kind: InteractionResponseType::DeferredChannelMessageWithSource,
                    data: None,
                };
                state
                    .bot
                    .http
                    .interaction(event.application_id)
                    .create_response(event.id, &event.token, &deferred)
                    .await?;

                let _ = state.event_tx.send(AppEvent::Custom {
                    name: "voice_joining".to_string(),
                    payload: serde_json::json!({
                        "guild_id": guild_id.get().to_string(),
                        "user_id": user_id.get().to_string(),
                        "message": "joining voice channel",
                    }),
                });
                let _ = state.event_tx.send(AppEvent::VoiceJoinRequested {
                    guild_id: guild_id.get().to_string(),
                    user_id: user_id.get().to_string(),
                    channel_id: channel_id.get().to_string(),
                });

                state
                    .set_voice_join_state(
                        guild_id,
                        VoiceJoinState::Joining {
                            guild_id: guild_id.get().to_string(),
                            user_id: user_id.get().to_string(),
                            channel_id: channel_id.get().to_string(),
                            message: "joining voice channel".to_string(),
                        },
                    )
                    .await;

                let state_clone = state.clone();
                let token = event.token.clone();
                let application_id = event.application_id;
                tokio::spawn(async move {
                    let result = voice::join_voice(&state_clone, guild_id, channel_id).await;
                    let failure_kind = result.as_ref().err().map(voice::classify_join_error);
                    let dave_required = result
                        .as_ref()
                        .err()
                        .is_some_and(dave::is_dave_required_join_error);
                    let (content, causes) = match result {
                        Ok(_) => ("✅ voice joined".to_string(), Vec::new()),
                        Err(err) => {
                            let causes = voice::join_error_causes(&err);
                            let detail = voice::describe_join_error(&err);
                            (format!("❌ {}", detail), causes)
                        }
                    };
                    if !causes.is_empty() {
                        tracing::warn!(
                            guild_id = %guild_id.get(),
                            user_id = %user_id.get(),
                            failure_kind = ?failure_kind,
                            causes = ?causes,
                            "voice join failed"
                        );
                        if dave_required || matches!(failure_kind, Some(voice::JoinFailureKind::RequiresDave)) {
                            tracing::error!(
                                guild_id = %guild_id.get(),
                                user_id = %user_id.get(),
                                "voice channel requires DAVE/E2EE support"
                            );
                        }
                        let join_state = if dave_required || matches!(failure_kind, Some(voice::JoinFailureKind::RequiresDave)) {
                            let capabilities = state_clone.voice_engine.voice_capabilities();
                            VoiceJoinState::Unsupported {
                                guild_id: guild_id.get().to_string(),
                                user_id: user_id.get().to_string(),
                                channel_id: channel_id.get().to_string(),
                                message: content.clone(),
                                failure_kind: failure_kind.map(|kind| kind.label().to_string()).unwrap_or_else(|| "Other".to_string()),
                                causes: causes.clone(),
                                dave_required: true,
                                engine_name: capabilities.engine_name.to_string(),
                                max_dave_protocol_version: capabilities.max_dave_protocol_version,
                            }
                        } else {
                            VoiceJoinState::Failed {
                                guild_id: guild_id.get().to_string(),
                                user_id: user_id.get().to_string(),
                                channel_id: channel_id.get().to_string(),
                                message: content.clone(),
                                causes: causes.clone(),
                            }
                        };
                        state_clone.set_voice_join_state(guild_id, join_state).await;
                        let _ = state_clone.event_tx.send(AppEvent::Custom {
                            name: "voice_join_error".to_string(),
                            payload: serde_json::json!({
                                "guild_id": guild_id.get().to_string(),
                                "user_id": user_id.get().to_string(),
                                "message": content,
                                "causes": causes,
                                "failure_kind": failure_kind.map(|kind| kind.label()),
                            }),
                        });
                        let _ = state_clone.event_tx.send(AppEvent::VoiceJoinResult {
                            guild_id: guild_id.get().to_string(),
                            user_id: user_id.get().to_string(),
                            ok: false,
                            message: content.clone(),
                        });
                    } else {
                        state_clone
                            .set_voice_join_state(
                                guild_id,
                                VoiceJoinState::Joined {
                                    guild_id: guild_id.get().to_string(),
                                    user_id: user_id.get().to_string(),
                                    channel_id: channel_id.get().to_string(),
                                    message: content.clone(),
                                },
                            )
                            .await;
                        let _ = state_clone.event_tx.send(AppEvent::VoiceJoinResult {
                            guild_id: guild_id.get().to_string(),
                            user_id: user_id.get().to_string(),
                            ok: true,
                            message: content.clone(),
                        });
                    }
                    let _ = state_clone
                        .bot
                        .http
                        .interaction(application_id)
                        .update_response(&token)
                        .content(Some(&content))
                        .await;
                });
                return Ok(());
            } else {
                "❌ please join a voice channel first".to_string()
            }
        }
        "leave" => {
            voice::leave_voice(&state, guild_id).await?;
            "✅ voice left".to_string()
        }
        "status" => {
            match state.voice_join_state(guild_id).await {
                VoiceJoinState::Idle { .. } => {
                    let session = state.voice_session(guild_id).await;
                    match session {
                        Some(VoiceSession { channel_id: Some(channel_id), .. }) => format!("connected to {}", channel_id),
                        _ => "not connected".to_string(),
                    }
                }
                VoiceJoinState::Joining { message, .. } => format!("joining: {}", message),
                VoiceJoinState::Joined { channel_id, message, .. } => format!("connected to {} ({})", channel_id, message),
                VoiceJoinState::Unsupported { message, .. } => format!("unsupported: {}", message),
                VoiceJoinState::Failed { message, .. } => format!("failed: {}", message),
            }
        }
        _ => "unknown voice command".to_string(),
    };

    let response = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(response_text),
            ..InteractionResponseData::default()
        }),
    };

    state
        .bot
        .http
        .interaction(event.application_id)
        .create_response(event.id, &event.token, &response)
        .await?;

    Ok(())
}

pub async fn handle_voice_state_update(state: AppState, event: &VoiceStateUpdate) {
    state.voice_engine.process_voice_state_update(&state, event).await;
}

pub async fn handle_voice_server_update(state: AppState, event: VoiceServerUpdate) {
    state.voice_engine.process_voice_server_update(&state, event).await;
}

async fn handle_voice_diag_command(state: AppState, event: &InteractionCreate, command: &twilight_model::application::interaction::application_command::CommandData) -> Result<()> {
    let Some(guild_id) = command.guild_id else { return Ok(()); };
    let diagnostics = state.voice_diagnostics(guild_id).await;
    let join_state_summary = match &diagnostics.join_state {
        VoiceJoinState::Idle { .. } => "idle".to_string(),
        VoiceJoinState::Joining { message, .. } => format!("joining: {message}"),
        VoiceJoinState::Joined { channel_id, message, .. } => format!("joined {channel_id}: {message}"),
        VoiceJoinState::Unsupported { message, engine_name, max_dave_protocol_version, .. } => format!("unsupported: {message} ({engine_name} / DAVE v{max_dave_protocol_version})"),
        VoiceJoinState::Failed { message, .. } => format!("failed: {message}"),
    };
    let signal = diagnostics.signal_trace.as_ref().map(|trace| format!("{} · {}", trace.stage, trace.message)).unwrap_or_else(|| "none".to_string());
    let receive = diagnostics.receive_trace.as_ref().map(|trace| format!("{} · {}", trace.kind, trace.message)).unwrap_or_else(|| "none".to_string());
    let ready_hint = match &diagnostics.signal_trace {
        Some(trace) if trace.stage == "dave_ready_hint" => trace.message.clone(),
        _ => "unknown".to_string(),
    };
    let content = format!(
        "engine={} dave={} v{} | join={} | signal={} | receive={} | dave_hint={}",
        diagnostics.voice_capabilities.engine_name,
        if diagnostics.voice_capabilities.supports_dave { "yes" } else { "no" },
        diagnostics.voice_capabilities.max_dave_protocol_version,
        join_state_summary,
        signal,
        receive,
        ready_hint,
    );
    let response = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(content),
            ..InteractionResponseData::default()
        }),
    };

    state
        .bot
        .http
        .interaction(event.application_id)
        .create_response(event.id, &event.token, &response)
        .await?;

    Ok(())
}

pub async fn handle_message_create(state: AppState, event: &MessageCreate) {
    if let Some(guild_id) = event.guild_id {
        let _ = state.event_tx.send(AppEvent::MessageCreate {
            guild_id: guild_id.get().to_string(),
            content: event.content.clone(),
            author: event.author.name.clone(),
        });
    }
}
