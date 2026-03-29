use crate::{
    dave,
    state::{AppEvent, AppState, VoiceCapabilities, VoiceJoinState, VoiceSession, VoiceSignalTrace, VoiceUserMeta},
    voice::VoiceReceiveHandler,
    voice,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use songbird::error::JoinResult;
use songbird::events::{CoreEvent, Event as SongbirdEvent};
use std::sync::Arc;
use twilight_model::{
    gateway::payload::incoming::{VoiceServerUpdate, VoiceStateUpdate},
    id::{marker::{ChannelMarker, GuildMarker}, Id},
};

#[async_trait]
pub trait VoiceEngine: Send + Sync {
    fn name(&self) -> &'static str;

    fn supports_dave(&self) -> bool {
        false
    }

    fn max_dave_protocol_version(&self) -> u16 {
        0
    }

    fn voice_capabilities(&self) -> VoiceCapabilities {
        dave::voice_capabilities(self.name(), self.supports_dave(), self.max_dave_protocol_version())
    }

    async fn process_voice_state_update(&self, state: &AppState, event: &VoiceStateUpdate);
    async fn process_voice_server_update(&self, state: &AppState, event: VoiceServerUpdate);
    async fn join(
        &self,
        state: &AppState,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
    ) -> JoinResult<VoiceSession>;
    async fn leave(&self, state: &AppState, guild_id: Id<GuildMarker>) -> Result<()>;
}

#[derive(Clone)]
pub struct SongbirdVoiceEngine {
    songbird: Arc<songbird::Songbird>,
}

impl SongbirdVoiceEngine {
    pub fn new(songbird: Arc<songbird::Songbird>) -> Self {
        Self { songbird }
    }

    async fn join_once(
        &self,
        state: &AppState,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
    ) -> JoinResult<VoiceSession> {
        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "join_start".to_string(),
                message: "joining voice channel".to_string(),
                user_id: None,
                channel_id: Some(channel_id.get().to_string()),
                ssrc: None,
            })
            .await;
        let call = self.songbird.join(guild_id, channel_id).await?;
        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "join_post_join".to_string(),
                message: format!("join returned call handle; dave_ready_hint will follow"),
                user_id: None,
                channel_id: Some(channel_id.get().to_string()),
                ssrc: None,
            })
            .await;
        let receive_handler = VoiceReceiveHandler::new(state.clone(), guild_id, state.event_tx.clone())
            .with_shared_metadata(state.voice_metadata_for_guild(guild_id).await);
        receive_handler.emit_dave_ready_hint(false, "join complete; waiting for decoded voice".to_string()).await;
        {
            let mut call_lock = call.lock().await;
            call_lock.add_global_event(SongbirdEvent::Core(CoreEvent::SpeakingStateUpdate), receive_handler.clone());
            call_lock.add_global_event(SongbirdEvent::Core(CoreEvent::RtpPacket), receive_handler.clone());
            call_lock.add_global_event(SongbirdEvent::Core(CoreEvent::VoiceTick), receive_handler.clone());
            call_lock.add_global_event(SongbirdEvent::Core(CoreEvent::ClientDisconnect), receive_handler.clone());
        }

        let session = VoiceSession {
            guild_id: guild_id.get().to_string(),
            channel_id: Some(channel_id.get().to_string()),
        };
        state.set_voice_session(guild_id, session.clone()).await;
        let _ = state.event_tx.send(AppEvent::Custom {
            name: "voice_joined".to_string(),
            payload: serde_json::json!({
                "guild_id": guild_id.get().to_string(),
                "channel_id": channel_id.get().to_string(),
            }),
        });
        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "join_ready".to_string(),
                message: "voice call joined".to_string(),
                user_id: None,
                channel_id: Some(channel_id.get().to_string()),
                ssrc: None,
            })
            .await;
        receive_handler.emit_dave_ready_hint(false, "waiting for DAVE handshake completion".to_string()).await;
        Ok(session)
    }
}

#[async_trait]
impl VoiceEngine for SongbirdVoiceEngine {
    fn name(&self) -> &'static str {
        dave::ENGINE_NAME
    }

    fn supports_dave(&self) -> bool {
        true
    }

    fn max_dave_protocol_version(&self) -> u16 {
        dave::MAX_DAVE_PROTOCOL_VERSION
    }

    async fn process_voice_state_update(&self, state: &AppState, event: &VoiceStateUpdate) {
        if let Some(guild_id) = event.guild_id {
            state
                .record_voice_signal_trace(VoiceSignalTrace {
                    guild_id: guild_id.get().to_string(),
                    stage: "voice_state_update".to_string(),
                    message: format!("channel={:?} session={}", event.channel_id.map(|id| id.get().to_string()), event.session_id),
                    user_id: Some(event.user_id.get().to_string()),
                    channel_id: event.channel_id.map(|id| id.get().to_string()),
                    ssrc: None,
                })
                .await;
            if event.channel_id.is_none() {
                state.remove_user_voice_state(guild_id, event.user_id).await;
                state.remove_voice_user_meta(guild_id, event.user_id).await;
            }

            let songbird_event = twilight_model::gateway::event::Event::VoiceStateUpdate(Box::new(event.clone()));
            self.songbird.process(&songbird_event).await;

            if let Some(channel_id) = event.channel_id {
                state.set_user_voice_state(guild_id, event.user_id, channel_id).await;
            } else {
                state.remove_user_voice_state(guild_id, event.user_id).await;
            }

            let meta = voice::voice_meta_from_voice_state(guild_id, event);
            if event.channel_id.is_some() {
                state.set_voice_user_meta(guild_id, event.user_id, meta.clone()).await;
            } else {
                state.remove_voice_user_meta(guild_id, event.user_id).await;
            }

            let _ = state.event_tx.send(AppEvent::VoiceStateUpdate {
                guild_id: guild_id.get().to_string(),
                user_id: event.user_id.get().to_string(),
                channel_id: event.channel_id.map(|id| id.get().to_string()),
                user_name: meta.user_name,
                display_name: meta.display_name,
                avatar_url: meta.avatar_url,
            });

            if event.channel_id.is_some() && event.member.is_none() {
                let state_clone = state.clone();
                let event_clone = event.clone();
                tokio::spawn(async move {
                    if let Some(guild_id) = event_clone.guild_id {
                        let meta: VoiceUserMeta = voice::resolve_voice_user_meta(&state_clone, guild_id, &event_clone).await;
                        let expected_channel = event_clone.channel_id.map(|id| id.get().to_string());
                        let should_publish = {
                            let current = state_clone.voice_metadata_for_guild(guild_id).await.lock().await.get(&event_clone.user_id).cloned();
                            let current_channel = current.as_ref().and_then(|m| m.channel_id.clone());
                            current_channel == expected_channel
                        };
                        if should_publish {
                            state_clone.set_voice_user_meta(guild_id, event_clone.user_id, meta.clone()).await;
                            let _ = state_clone.event_tx.send(AppEvent::VoiceStateUpdate {
                                guild_id: guild_id.get().to_string(),
                                user_id: event_clone.user_id.get().to_string(),
                                channel_id: event_clone.channel_id.map(|id| id.get().to_string()),
                                user_name: meta.user_name,
                                display_name: meta.display_name,
                                avatar_url: meta.avatar_url,
                            });
                        }
                    }
                });
            }
        }
    }

    async fn process_voice_server_update(&self, state: &AppState, event: VoiceServerUpdate) {
        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: event.guild_id.get().to_string(),
                stage: "voice_server_update".to_string(),
                message: format!("endpoint={}", event.endpoint.clone().unwrap_or_else(|| "<none>".to_string())),
                user_id: None,
                channel_id: None,
                ssrc: None,
            })
            .await;
        let songbird_event = twilight_model::gateway::event::Event::VoiceServerUpdate(event);
        self.songbird.process(&songbird_event).await;
    }

    async fn join(
        &self,
        state: &AppState,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
    ) -> JoinResult<VoiceSession> {
        match self.join_once(state, guild_id, channel_id).await {
            Ok(session) => Ok(session),
            Err(err) if matches!(voice::classify_join_error(&err), voice::JoinFailureKind::DriverConnection) => {
                let detail = voice::describe_join_error(&err);
                tracing::warn!(
                    guild_id = %guild_id.get(),
                    channel_id = %channel_id.get(),
                    error = %err,
                    detail = %detail,
                    "voice join driver error, retrying after leave"
                );
                let _ = self.songbird.remove(guild_id).await;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                self.join_once(state, guild_id, channel_id).await
            }
            Err(err) => Err(err),
        }
    }

    async fn leave(&self, state: &AppState, guild_id: Id<GuildMarker>) -> Result<()> {
        let _ = state.event_tx.send(AppEvent::VoiceSignalTrace {
            trace: VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "leave_start".to_string(),
                message: "leaving voice channel".to_string(),
                user_id: None,
                channel_id: state.voice_session(guild_id).await.and_then(|s| s.channel_id.clone()),
                ssrc: None,
            },
        });
        self.songbird.leave(guild_id).await.context("failed to leave voice channel")?;
        state.remove_voice_session(guild_id).await;
        state.clear_guild_voice_state(guild_id).await;
        state
            .set_voice_join_state(
                guild_id,
                VoiceJoinState::Idle { guild_id: guild_id.get().to_string() },
            )
            .await;
        let _ = state.event_tx.send(AppEvent::Custom {
            name: "voice_left".to_string(),
            payload: serde_json::json!({
                "guild_id": guild_id.get().to_string(),
            }),
        });
        let _ = state.event_tx.send(AppEvent::VoiceSignalTrace {
            trace: VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "leave_complete".to_string(),
                message: "voice channel left".to_string(),
                user_id: None,
                channel_id: None,
                ssrc: None,
            },
        });
        Ok(())
    }
}
