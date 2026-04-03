use crate::{auth::AuthSession, config::Config, voice_engine::VoiceEngine};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, Mutex};
use twilight_gateway::MessageSender;
use twilight_http::Client;
use twilight_model::id::{
    marker::{ChannelMarker, GuildMarker, UserMarker},
    Id,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub bot: Arc<BotState>,
    pub activity: Arc<Mutex<ActivityState>>,
    pub event_tx: broadcast::Sender<AppEvent>,
    pub ready_tx: broadcast::Sender<BotReady>,
    pub ready_state: Arc<Mutex<Option<BotReady>>>,
    pub voice_engine: Arc<dyn VoiceEngine>,
}

pub struct BotState {
    pub http: Arc<Client>,
    pub gateway: MessageSender,
    pub bot_user_id: Id<UserMarker>,
    pub voice_sessions: Mutex<HashMap<Id<GuildMarker>, VoiceSession>>,
    pub user_voice_states: Mutex<HashMap<Id<GuildMarker>, HashMap<Id<UserMarker>, Id<ChannelMarker>>>>,
    pub voice_user_meta: Mutex<HashMap<Id<GuildMarker>, Arc<Mutex<HashMap<Id<UserMarker>, VoiceUserMeta>>>>>,
    pub voice_join_states: Mutex<HashMap<Id<GuildMarker>, VoiceJoinState>>,
    pub last_voice_receive_traces: Mutex<HashMap<String, VoiceReceiveTrace>>,
    pub last_voice_signal_traces: Mutex<HashMap<String, VoiceSignalTrace>>,
    pub auth_sessions: Mutex<HashMap<String, AuthSession>>,
    pub bot_pending_session_id: Mutex<HashMap<Id<GuildMarker>, String>>,
    pub pending_voice_info: Mutex<HashMap<Id<GuildMarker>, PendingVoiceInfo>>,
    pub metrics: Mutex<VoiceMetrics>,
}

/// Simple voice metrics counters
#[derive(Clone, Default, Debug)]
pub struct VoiceMetrics {
    pub total_joins: u64,
    pub total_leaves: u64,
    pub total_voice_frames: u64,
    pub total_dave_decrypted: u64,
    pub total_passthrough_frames: u64,
    pub total_reconnects: u64,
    pub total_heartbeat_acks: u64,
    pub total_heartbeat_timeouts: u64,
    pub current_active_connections: u64,
}

#[derive(Clone, Debug)]
pub struct PendingVoiceInfo {
    pub session_id: String,
    pub token: String,
    pub endpoint: String,
}

pub struct ActivityState {
    pub sessions: HashMap<String, ActivitySession>,
}

#[derive(Clone, Serialize, Default, Debug)]
pub struct ActivitySession {
    pub guild_id: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum AppEvent {
    VoiceStateUpdate {
        guild_id: String,
        user_id: String,
        channel_id: Option<String>,
        user_name: Option<String>,
        display_name: Option<String>,
        avatar_url: Option<String>,
    },
    VoiceSpeaking {
        guild_id: String,
        user_id: String,
        channel_id: Option<String>,
        user_name: Option<String>,
        display_name: Option<String>,
        avatar_url: Option<String>,
        ssrc: u32,
        speaking: bool,
    },
    VoiceAudioFrame {
        guild_id: String,
        user_id: String,
        ssrc: u32,
        samples: usize,
    },
    VoiceStream {
        guild_id: String,
        users: Vec<VoiceStreamUser>,
        audio_frames: usize,
    },
    VoiceReceiveTrace {
        trace: VoiceReceiveTrace,
    },
    VoiceSignalTrace {
        trace: VoiceSignalTrace,
    },
    VoiceJoinState {
        state: VoiceJoinState,
    },
    VoiceJoinRequested {
        guild_id: String,
        user_id: String,
        channel_id: String,
    },
    VoiceJoinResult {
        guild_id: String,
        user_id: String,
        ok: bool,
        message: String,
    },
    MessageCreate {
        guild_id: String,
        content: String,
        author: String,
    },
    Custom {
        name: String,
        payload: serde_json::Value,
    },
    VoiceSessionReady {
        guild_id: String,
        session_id: String,
        token: String,
        endpoint: String,
    },
}

#[derive(Clone, Serialize, Default)]
pub struct BotReady {
    pub status: String,
    pub application_id: String,
    pub guild_id: Option<String>,
    pub commands: Vec<String>,
    pub voice_capabilities: VoiceCapabilities,
}

#[derive(Clone, Default, Serialize, Debug)]
pub struct VoiceSession {
    pub guild_id: String,
    pub channel_id: Option<String>,
}

#[derive(Clone, Serialize, Default, Debug)]
pub struct VoiceReceiveTrace {
    pub guild_id: String,
    pub kind: String,
    pub message: String,
    pub user_id: Option<String>,
    pub ssrc: Option<u32>,
    pub sequence: Option<u16>,
    pub timestamp: Option<u32>,
    pub payload_len: Option<usize>,
    pub payload_offset: Option<usize>,
    pub payload_end_pad: Option<usize>,
    pub has_dave_marker: Option<bool>,
    pub decoded_users: Option<usize>,
    pub silent_users: Option<usize>,
    pub audio_frames: Option<usize>,
    pub decoded_samples: Option<usize>,
}

#[derive(Clone, Serialize, Default, Debug)]
pub struct VoiceSignalTrace {
    pub guild_id: String,
    pub stage: String,
    pub message: String,
    pub user_id: Option<String>,
    pub channel_id: Option<String>,
    pub ssrc: Option<u32>,
}

#[derive(Clone, Copy, Serialize, Default, Debug)]
pub struct VoiceCapabilities {
    pub engine_name: &'static str,
    pub supports_dave: bool,
    pub max_dave_protocol_version: u16,
}

#[derive(Clone, Serialize, Debug)]
pub struct VoiceDiagnostics {
    pub guild_id: String,
    pub voice: Option<VoiceSession>,
    pub join_state: VoiceJoinState,
    pub voice_capabilities: VoiceCapabilities,
    pub signal_trace: Option<VoiceSignalTrace>,
    pub receive_trace: Option<VoiceReceiveTrace>,
}

#[derive(Clone, Serialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VoiceJoinState {
    Idle {
        guild_id: String,
    },
    Joining {
        guild_id: String,
        user_id: String,
        channel_id: String,
        message: String,
    },
    Joined {
        guild_id: String,
        user_id: String,
        channel_id: String,
        message: String,
    },
    Unsupported {
        guild_id: String,
        user_id: String,
        channel_id: String,
        message: String,
        failure_kind: String,
        causes: Vec<String>,
        dave_required: bool,
        engine_name: String,
        max_dave_protocol_version: u16,
    },
    Failed {
        guild_id: String,
        user_id: String,
        channel_id: String,
        message: String,
        causes: Vec<String>,
    },
}

impl Default for VoiceJoinState {
    fn default() -> Self {
        Self::Idle {
            guild_id: String::new(),
        }
    }
}

#[derive(Clone, Default, Serialize, Debug)]
pub struct VoiceUserMeta {
    pub user_name: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Clone, Serialize, Default, Debug)]
pub struct VoiceStreamUser {
    pub user_id: String,
    pub user_name: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub channel_id: Option<String>,
    pub speaking: bool,
    pub ssrc: Option<u32>,
    pub samples: Option<usize>,
}

impl AppState {
    pub fn new(config: Config, http: Arc<Client>, gateway: MessageSender, bot_user_id: Id<UserMarker>, voice_engine: Arc<dyn VoiceEngine>) -> Self {
        let config = Arc::new(config);
        let bot = Arc::new(BotState {
            http,
            gateway,
            bot_user_id,
            voice_sessions: Mutex::new(HashMap::new()),
            user_voice_states: Mutex::new(HashMap::new()),
            voice_user_meta: Mutex::new(HashMap::new()),
            voice_join_states: Mutex::new(HashMap::new()),
            last_voice_receive_traces: Mutex::new(HashMap::new()),
            last_voice_signal_traces: Mutex::new(HashMap::new()),
            auth_sessions: Mutex::new(HashMap::new()),
            pending_voice_info: Mutex::new(HashMap::new()),
            bot_pending_session_id: Mutex::new(HashMap::new()),
            metrics: Mutex::new(VoiceMetrics::default()),
        });
        let activity = Arc::new(Mutex::new(ActivityState {
            sessions: HashMap::new(),
        }));
        let (event_tx, _) = broadcast::channel(100);
        let (ready_tx, _) = broadcast::channel(16);
        let ready_state = Arc::new(Mutex::new(None));
        Self {
            config,
            bot,
            activity,
            event_tx,
            ready_tx,
            ready_state,
            voice_engine,
        }
    }

    pub async fn set_voice_join_state(&self, guild_id: Id<GuildMarker>, join_state: VoiceJoinState) {
        let mut voice_join_states = self.bot.voice_join_states.lock().await;
        match &join_state {
            VoiceJoinState::Idle { .. } => {
                voice_join_states.remove(&guild_id);
            }
            _ => {
                voice_join_states.insert(guild_id, join_state.clone());
            }
        }

        let _ = self.event_tx.send(AppEvent::VoiceJoinState { state: join_state });
    }

    pub async fn voice_session(&self, guild_id: Id<GuildMarker>) -> Option<VoiceSession> {
        self.bot.voice_sessions.lock().await.get(&guild_id).cloned()
    }

    pub async fn set_voice_session(&self, guild_id: Id<GuildMarker>, session: VoiceSession) {
        self.bot.voice_sessions.lock().await.insert(guild_id, session);
    }

    pub async fn remove_voice_session(&self, guild_id: Id<GuildMarker>) {
        self.bot.voice_sessions.lock().await.remove(&guild_id);
    }

    /// Check if there's an active voice session for a guild (by string ID)
    pub async fn voice_session_by_guild_id(&self, guild_id_str: &str) -> Option<VoiceSession> {
        let sessions = self.bot.voice_sessions.lock().await;
        sessions.values().find(|s| s.guild_id == guild_id_str).cloned()
    }

    pub async fn voice_metrics(&self) -> VoiceMetrics {
        self.bot.metrics.lock().await.clone()
    }

    pub async fn increment_voice_metric(&self, metric: impl FnOnce(&mut VoiceMetrics)) {
        let mut metrics = self.bot.metrics.lock().await;
        metric(&mut *metrics);
    }

    pub async fn voice_metadata_for_guild(&self, guild_id: Id<GuildMarker>) -> Arc<Mutex<HashMap<Id<UserMarker>, VoiceUserMeta>>> {
        let mut voice_user_meta = self.bot.voice_user_meta.lock().await;
        voice_user_meta
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(HashMap::new())))
            .clone()
    }

    pub async fn set_user_voice_state(&self, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>, channel_id: Id<ChannelMarker>) {
        let mut states = self.bot.user_voice_states.lock().await;
        states.entry(guild_id).or_default().insert(user_id, channel_id);
    }

    pub async fn remove_user_voice_state(&self, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) {
        let mut states = self.bot.user_voice_states.lock().await;
        if let Some(guild_states) = states.get_mut(&guild_id) {
            guild_states.remove(&user_id);
            if guild_states.is_empty() {
                states.remove(&guild_id);
            }
        }
    }

    pub async fn user_voice_channel(&self, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) -> Option<Id<ChannelMarker>> {
        self.bot
            .user_voice_states
            .lock()
            .await
            .get(&guild_id)
            .and_then(|states| states.get(&user_id).copied())
    }

    pub async fn set_voice_user_meta(&self, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>, meta: VoiceUserMeta) {
        let metadata = self.voice_metadata_for_guild(guild_id).await;
        metadata.lock().await.insert(user_id, meta);
    }

    pub async fn remove_voice_user_meta(&self, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) {
        let voice_user_meta = self.bot.voice_user_meta.lock().await;
        if let Some(metadata) = voice_user_meta.get(&guild_id) {
            metadata.lock().await.remove(&user_id);
        }
    }

    pub async fn clear_guild_voice_state(&self, guild_id: Id<GuildMarker>) {
        self.bot.user_voice_states.lock().await.remove(&guild_id);
        self.bot.voice_user_meta.lock().await.remove(&guild_id);
    }

    pub async fn auth_session_guilds(&self, session_id: &str) -> Option<Vec<String>> {
        self.bot.auth_sessions.lock().await.get(session_id).map(|session| session.guild_ids.clone())
    }

    pub async fn is_session_authorized_for_guild(&self, session_id: &str, guild_id: &str) -> bool {
        self.bot
            .auth_sessions
            .lock()
            .await
            .get(session_id)
            .map(|session| session.guild_ids.iter().any(|allowed| allowed == guild_id))
            .unwrap_or(false)
    }

    pub async fn store_auth_session(&self, session: AuthSession) {
        self.bot.auth_sessions.lock().await.insert(session.session_id.clone(), session);
    }

    pub async fn auth_session(&self, session_id: &str) -> Option<AuthSession> {
        self.bot.auth_sessions.lock().await.get(session_id).cloned()
    }

    pub async fn store_pending_voice_info(&self, guild_id: Id<GuildMarker>, token: String, endpoint: String) {
        let session_id = self.user_voice_channel(guild_id, self.bot.bot_user_id)
            .await
            .map(|_| "active".to_string())
            .unwrap_or_else(|| "pending".to_string());

        self.bot.pending_voice_info.lock().await.insert(
            guild_id,
            PendingVoiceInfo {
                session_id,
                token,
                endpoint,
            },
        );
    }

    pub async fn take_pending_voice_info(&self, guild_id: Id<GuildMarker>) -> Option<PendingVoiceInfo> {
        self.bot.pending_voice_info.lock().await.remove(&guild_id)
    }

    pub async fn record_voice_receive_trace(&self, trace: VoiceReceiveTrace) {
        tracing::debug!(
            guild_id = %trace.guild_id,
            kind = %trace.kind,
            message = %trace.message,
            user_id = ?trace.user_id,
            ssrc = ?trace.ssrc,
            sequence = ?trace.sequence,
            timestamp = ?trace.timestamp,
            payload_len = ?trace.payload_len,
            payload_offset = ?trace.payload_offset,
            payload_end_pad = ?trace.payload_end_pad,
            has_dave_marker = ?trace.has_dave_marker,
            decoded_users = ?trace.decoded_users,
            silent_users = ?trace.silent_users,
            audio_frames = ?trace.audio_frames,
            decoded_samples = ?trace.decoded_samples,
            "voice receive trace",
        );
        if trace.kind == "decoded_voice" {
            tracing::info!(
                guild_id = %trace.guild_id,
                user_id = ?trace.user_id,
                ssrc = ?trace.ssrc,
                decoded_samples = ?trace.decoded_samples,
                audio_frames = ?trace.audio_frames,
                "voice decoded voice"
            );
        } else if trace.kind == "decoded_voice_empty" {
            tracing::warn!(
                guild_id = %trace.guild_id,
                user_id = ?trace.user_id,
                ssrc = ?trace.ssrc,
                audio_frames = ?trace.audio_frames,
                "voice decoded empty voice"
            );
        }
        self.bot
            .last_voice_receive_traces
            .lock()
            .await
            .insert(trace.guild_id.clone(), trace.clone());
        let _ = self.event_tx.send(AppEvent::VoiceReceiveTrace { trace });
    }

    pub async fn voice_receive_trace(&self, guild_id: Id<GuildMarker>) -> Option<VoiceReceiveTrace> {
        self.bot.last_voice_receive_traces.lock().await.get(&guild_id.get().to_string()).cloned()
    }

    pub async fn record_voice_signal_trace(&self, trace: VoiceSignalTrace) {
        match trace.stage.as_str() {
            "voice_state_update" | "voice_server_update" | "join_start" | "join_post_join" | "join_ready" | "driver_connect" | "driver_reconnect" | "driver_disconnect" | "leave_start" | "leave_complete" | "speaking_state_update" => {
                tracing::info!(
                    guild_id = %trace.guild_id,
                    stage = %trace.stage,
                    message = %trace.message,
                    user_id = ?trace.user_id,
                    channel_id = ?trace.channel_id,
                    ssrc = ?trace.ssrc,
                    "voice signal trace",
                );
            }
            "dave_ready_hint" => {
                tracing::info!(
                    guild_id = %trace.guild_id,
                    stage = %trace.stage,
                    message = %trace.message,
                    "voice signal trace",
                );
            }
            _ => {
                tracing::debug!(
                    guild_id = %trace.guild_id,
                    stage = %trace.stage,
                    message = %trace.message,
                    user_id = ?trace.user_id,
                    channel_id = ?trace.channel_id,
                    ssrc = ?trace.ssrc,
                    "voice signal trace",
                );
            }
        }
        if trace.stage == "dave_ready_hint" {
            tracing::info!(
                guild_id = %trace.guild_id,
                message = %trace.message,
                "DAVE readiness hint"
            );
        }
        self.bot
            .last_voice_signal_traces
            .lock()
            .await
            .insert(trace.guild_id.clone(), trace.clone());
        let _ = self.event_tx.send(AppEvent::VoiceSignalTrace { trace });
    }

    pub async fn voice_signal_trace(&self, guild_id: Id<GuildMarker>) -> Option<VoiceSignalTrace> {
        self.bot.last_voice_signal_traces.lock().await.get(&guild_id.get().to_string()).cloned()
    }

    pub async fn voice_join_state(&self, guild_id: Id<GuildMarker>) -> VoiceJoinState {
        self.bot
            .voice_join_states
            .lock()
            .await
            .get(&guild_id)
            .cloned()
            .unwrap_or(VoiceJoinState::Idle {
                guild_id: guild_id.get().to_string(),
            })
    }

    pub async fn voice_diagnostics(&self, guild_id: Id<GuildMarker>) -> VoiceDiagnostics {
        let voice = self.bot.voice_sessions.lock().await.get(&guild_id).cloned();
        let join_state = self.voice_join_state(guild_id).await;
        let voice_capabilities = self.voice_engine.voice_capabilities();
        let key = guild_id.get().to_string();
        let signal_trace = self.bot.last_voice_signal_traces.lock().await.get(&key).cloned();
        let receive_trace = self.bot.last_voice_receive_traces.lock().await.get(&key).cloned();

        VoiceDiagnostics {
            guild_id: guild_id.get().to_string(),
            voice,
            join_state,
            voice_capabilities,
            signal_trace,
            receive_trace,
        }
    }
}
