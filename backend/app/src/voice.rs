use crate::{
    dave,
    state::{AppEvent, AppState, VoiceReceiveTrace, VoiceSignalTrace, VoiceSession, VoiceStreamUser, VoiceUserMeta},
};
use anyhow::Result;
use songbird::error::{JoinError, JoinResult};
use songbird::events::{Event as SongbirdEvent, EventContext, EventHandler as SongbirdEventHandler};
use std::{collections::HashMap, error::Error as StdError, sync::Arc, time::{Duration, Instant}};
use tokio::sync::Mutex;
use twilight_model::{
    guild::Member,
    gateway::payload::incoming::VoiceStateUpdate,
    id::{marker::{ChannelMarker, GuildMarker, UserMarker}, Id},
};

pub type SpeakerId = Id<UserMarker>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinFailureKind {
    RequiresDave,
    DriverConnection,
    Other,
}

impl JoinFailureKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RequiresDave => "RequiresDave",
            Self::DriverConnection => "DriverConnection",
            Self::Other => "Other",
        }
    }
}

#[derive(Clone)]
pub struct VoiceReceiveHandler {
    pub state: AppState,
    pub guild_id: Id<GuildMarker>,
    pub activity_tx: tokio::sync::broadcast::Sender<AppEvent>,
    pub audio_buffers: Arc<Mutex<HashMap<u32, Vec<i16>>>>,
    pub ssrc_to_user: Arc<Mutex<HashMap<u32, SpeakerId>>>,
    pub voice_metadata: Arc<Mutex<HashMap<SpeakerId, VoiceUserMeta>>>,
    pub speaking_state: Arc<Mutex<HashMap<SpeakerId, bool>>>,
    pub active_frames: Arc<Mutex<usize>>,
    pub dave_ready_logged: Arc<Mutex<bool>>,
    pub last_stream_snapshot_at: Arc<Mutex<Instant>>,
}

#[derive(Clone, Debug, Default)]
pub struct PacketTrace {
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

impl VoiceReceiveHandler {
    pub fn new(state: AppState, guild_id: Id<GuildMarker>, activity_tx: tokio::sync::broadcast::Sender<AppEvent>) -> Self {
        Self {
            state,
            guild_id,
            activity_tx,
            audio_buffers: Arc::new(Mutex::new(HashMap::new())),
            ssrc_to_user: Arc::new(Mutex::new(HashMap::new())),
            voice_metadata: Arc::new(Mutex::new(HashMap::new())),
            speaking_state: Arc::new(Mutex::new(HashMap::new())),
            active_frames: Arc::new(Mutex::new(0)),
            dave_ready_logged: Arc::new(Mutex::new(false)),
            last_stream_snapshot_at: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1))),
        }
    }

    pub fn with_shared_metadata(mut self, shared: Arc<Mutex<HashMap<SpeakerId, VoiceUserMeta>>>) -> Self {
        self.voice_metadata = shared;
        self
    }

    pub async fn publish_stream_snapshot(&self, force: bool) {
        if !force {
            let mut last = self.last_stream_snapshot_at.lock().await;
            let now = Instant::now();
            if now.duration_since(*last) < Duration::from_millis(100) {
                return;
            }
            *last = now;
        } else {
            *self.last_stream_snapshot_at.lock().await = Instant::now();
        }

        let voice_metadata = self.voice_metadata.lock().await.clone();
        let ssrc_to_user = self.ssrc_to_user.lock().await.clone();
        let buffers = self.audio_buffers.lock().await;
        let speaking_state = self.speaking_state.lock().await.clone();
        let frames = *self.active_frames.lock().await;

        let users = voice_metadata
            .into_iter()
            .filter(|(_, meta)| meta.channel_id.is_some())
            .map(|(user_id, meta)| {
                let ssrc = ssrc_to_user.iter().find_map(|(ssrc, mapped)| (*mapped == user_id).then_some(*ssrc));
                let samples = ssrc.and_then(|ssrc| buffers.get(&ssrc).map(|buf| buf.len()));
                VoiceStreamUser {
                    user_id: user_id.get().to_string(),
                    user_name: meta.user_name,
                    display_name: meta.display_name,
                    avatar_url: meta.avatar_url,
                    channel_id: meta.channel_id,
                    speaking: speaking_state.get(&user_id).copied().unwrap_or_else(|| ssrc.is_some()),
                    ssrc,
                    samples,
                }
            })
            .collect::<Vec<_>>();

        let _ = self.activity_tx.send(AppEvent::VoiceStream {
            guild_id: self.guild_id.get().to_string(),
            users,
            audio_frames: frames,
        });
    }

    async fn emit_receive_trace(&self, trace: VoiceReceiveTrace) {
        self.state.record_voice_receive_trace(trace).await;
    }

    async fn emit_signal_trace(&self, trace: VoiceSignalTrace) {
        self.state.record_voice_signal_trace(trace).await;
    }

    pub async fn trace_packet(&self, trace: PacketTrace) {
        self.emit_receive_trace(VoiceReceiveTrace {
            guild_id: trace.guild_id,
            kind: trace.kind,
            message: trace.message,
            user_id: trace.user_id,
            ssrc: trace.ssrc,
            sequence: trace.sequence,
            timestamp: trace.timestamp,
            payload_len: trace.payload_len,
            payload_offset: trace.payload_offset,
            payload_end_pad: trace.payload_end_pad,
            has_dave_marker: trace.has_dave_marker,
            decoded_users: trace.decoded_users,
            silent_users: trace.silent_users,
            audio_frames: trace.audio_frames,
            decoded_samples: trace.decoded_samples,
        })
        .await;
    }

    pub async fn emit_dave_ready_hint(&self, is_ready: bool, message: String) {
        self.emit_signal_trace(VoiceSignalTrace {
            guild_id: self.guild_id.get().to_string(),
            stage: "dave_ready_hint".to_string(),
            message: format!("ready={is_ready} {message}"),
            user_id: None,
            channel_id: None,
            ssrc: None,
        })
        .await;
    }

    pub async fn set_voice_meta(&self, user_id: SpeakerId, meta: VoiceUserMeta) {
        self.voice_metadata.lock().await.insert(user_id, meta);
    }
}

pub fn voice_meta_from_voice_state(guild_id: Id<GuildMarker>, event: &VoiceStateUpdate) -> VoiceUserMeta {
    event
        .member
        .as_ref()
        .map(|member| voice_meta_from_member(guild_id, event.user_id, event.channel_id, member))
        .unwrap_or_else(|| VoiceUserMeta {
            user_name: Some(event.user_id.get().to_string()),
            display_name: Some(event.user_id.get().to_string()),
            avatar_url: None,
            channel_id: event.channel_id.map(|id| id.get().to_string()),
        })
}

pub async fn resolve_voice_user_meta(state: &AppState, guild_id: Id<GuildMarker>, event: &VoiceStateUpdate) -> VoiceUserMeta {
    let member = if let Some(member) = event.member.clone() {
        Some(member)
    } else {
        fetch_member(state, guild_id, event.user_id).await
    };

    member
        .as_ref()
        .map(|member| voice_meta_from_member(guild_id, event.user_id, event.channel_id, member))
        .unwrap_or_else(|| voice_meta_from_voice_state(guild_id, event))
}

async fn fetch_member(state: &AppState, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) -> Option<Member> {
    state
        .bot
        .http
        .guild_member(guild_id, user_id)
        .await
        .ok()?
        .model()
        .await
        .ok()
}

fn voice_meta_from_member(guild_id: Id<GuildMarker>, user_id: Id<UserMarker>, channel_id: Option<Id<ChannelMarker>>, member: &Member) -> VoiceUserMeta {
    let display_name = member
        .nick
        .clone()
        .or_else(|| member.user.global_name.clone())
        .or_else(|| Some(member.user.name.clone()));

    let avatar_url = if let Some(hash) = member.avatar.as_ref() {
        Some(format!("https://cdn.discordapp.com/guilds/{}/users/{}/avatars/{}.png?size=128", guild_id.get(), user_id.get(), hash))
    } else if let Some(hash) = member.user.avatar.as_ref() {
        Some(format!("https://cdn.discordapp.com/avatars/{}/{}.png?size=128", user_id.get(), hash))
    } else {
        Some(format!("https://cdn.discordapp.com/embed/avatars/{}.png", (member.user.discriminator().get() % 5)))
    };

    VoiceUserMeta {
        user_name: Some(member.user.name.clone()),
        display_name,
        avatar_url,
        channel_id: channel_id.map(|id| id.get().to_string()),
    }
}

#[async_trait::async_trait]
impl SongbirdEventHandler for VoiceReceiveHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<SongbirdEvent> {
        match ctx {
            EventContext::RtpPacket(rtp_packet) => {
                let rtp = rtp_packet.rtp();
                let payload = rtp_packet.packet.as_ref();
                let has_dave_marker = dave::packet_has_dave_marker(payload);
                let sequence: u16 = rtp.get_sequence().into();
                let timestamp: u32 = rtp.get_timestamp().into();
                self.trace_packet(PacketTrace {
                    guild_id: self.guild_id.get().to_string(),
                    kind: if has_dave_marker { "rtp_packet_dave".to_string() } else { "rtp_packet".to_string() },
                    message: if has_dave_marker { "transport packet with dave marker".to_string() } else { "transport packet".to_string() },
                    user_id: None,
                    ssrc: Some(rtp.get_ssrc()),
                    sequence: Some(sequence),
                    timestamp: Some(timestamp),
                    payload_len: Some(payload.len()),
                    payload_offset: Some(rtp_packet.payload_offset),
                    payload_end_pad: Some(rtp_packet.payload_end_pad),
                    has_dave_marker: Some(has_dave_marker),
                    decoded_users: None,
                    silent_users: None,
                    audio_frames: None,
                    decoded_samples: None,
                })
                .await;
            }
            EventContext::SpeakingStateUpdate(speaking) => {
                if let Some(user_id) = speaking.user_id {
                    let user_id = Id::new(user_id.0);
                    let speaking_now = speaking.speaking.microphone() || speaking.speaking.soundshare() || speaking.speaking.priority();
                    self.emit_signal_trace(VoiceSignalTrace {
                        guild_id: self.guild_id.get().to_string(),
                        stage: "speaking_state_update".to_string(),
                        message: format!("speaking={speaking_now}"),
                        user_id: Some(user_id.get().to_string()),
                        channel_id: None,
                        ssrc: Some(speaking.ssrc),
                    }).await;
                    {
                        let mut ssrc_to_user = self.ssrc_to_user.lock().await;
                        ssrc_to_user.retain(|_, mapped| *mapped != user_id);
                        if speaking_now {
                            ssrc_to_user.insert(speaking.ssrc, user_id);
                        }
                    }
                    self.speaking_state.lock().await.insert(user_id, speaking_now);
                    let meta = self.voice_metadata.lock().await.get(&user_id).cloned().unwrap_or_default();
                    self.publish_stream_snapshot(true).await;
                    let _ = self.activity_tx.send(AppEvent::VoiceSpeaking {
                        guild_id: self.guild_id.get().to_string(),
                        user_id: user_id.get().to_string(),
                        channel_id: meta.channel_id,
                        user_name: meta.user_name,
                        display_name: meta.display_name,
                        avatar_url: meta.avatar_url,
                        ssrc: speaking.ssrc,
                        speaking: speaking_now,
                    });
                }
            }
            EventContext::VoiceTick(tick) => {
                let ssrc_map = self.ssrc_to_user.lock().await.clone();
                let mut changed = false;
                let mut snapshot_changed = false;
                self.emit_receive_trace(VoiceReceiveTrace {
                    guild_id: self.guild_id.get().to_string(),
                    kind: "voice_tick".to_string(),
                    message: "voice tick received".to_string(),
                    user_id: None,
                    ssrc: None,
                    sequence: None,
                    timestamp: None,
                    payload_len: None,
                    payload_offset: None,
                    payload_end_pad: None,
                    has_dave_marker: None,
                    decoded_users: Some(tick.speaking.len()),
                    silent_users: Some(tick.silent.len()),
                    audio_frames: Some(*self.active_frames.lock().await),
                    decoded_samples: None,
                }).await;
                {
                    let mut speaking_state = self.speaking_state.lock().await;
                    for ssrc in &tick.silent {
                        if let Some(&user_id) = ssrc_map.get(ssrc) {
                            if speaking_state.insert(user_id, false) != Some(false) {
                                snapshot_changed = true;
                            }
                        }
                    }
                    for ssrc in tick.speaking.keys() {
                        if let Some(&user_id) = ssrc_map.get(ssrc) {
                            if speaking_state.insert(user_id, true) != Some(true) {
                                snapshot_changed = true;
                            }
                        }
                    }
                }
                for (ssrc, voice_data) in tick.speaking.iter() {
                    if let Some(audio) = &voice_data.decoded_voice {
                        let samples: Vec<i16> = audio.clone();
                        if let Some(&user_id) = ssrc_map.get(ssrc) {
                            if !samples.is_empty() {
                                let mut logged = self.dave_ready_logged.lock().await;
                                if !*logged {
                                    *logged = true;
                                    self.emit_dave_ready_hint(true, format!("decoded_voice observed for ssrc {}", ssrc)).await;
                                }
                                let trace = VoiceReceiveTrace {
                                    guild_id: self.guild_id.get().to_string(),
                                    kind: "decoded_voice".to_string(),
                                    message: "opus decoded successfully".to_string(),
                                    user_id: Some(user_id.get().to_string()),
                                    ssrc: Some(*ssrc),
                                    sequence: None,
                                    timestamp: None,
                                    payload_len: Some(samples.len() * std::mem::size_of::<i16>()),
                                    payload_offset: None,
                                    payload_end_pad: None,
                                    has_dave_marker: None,
                                    decoded_users: None,
                                    silent_users: None,
                                    audio_frames: Some(*self.active_frames.lock().await),
                                    decoded_samples: Some(samples.len()),
                                };
                                tracing::info!(
                                    guild_id = %trace.guild_id,
                                    user_id = ?trace.user_id,
                                    ssrc = ?trace.ssrc,
                                    decoded_samples = ?trace.decoded_samples,
                                    audio_frames = ?trace.audio_frames,
                                    "voice decoded voice"
                                );
                                self.emit_receive_trace(trace).await;
                                let _ = self.activity_tx.send(AppEvent::VoiceAudioFrame {
                                    guild_id: self.guild_id.get().to_string(),
                                    user_id: user_id.get().to_string(),
                                    ssrc: *ssrc,
                                    samples: samples.len(),
                                });
                                let mut buffers = self.audio_buffers.lock().await;
                                let buffer = buffers.entry(*ssrc).or_default();
                                buffer.extend_from_slice(&samples);
                                if buffer.len() > 48_000 * 10 {
                                    let drain = buffer.len() - 48_000 * 5;
                                    buffer.drain(..drain);
                                }
                                *self.active_frames.lock().await += 1;
                                changed = true;
                            } else {
                                self.trace_packet(PacketTrace {
                                    guild_id: self.guild_id.get().to_string(),
                                    kind: "decoded_voice_empty".to_string(),
                                    message: "decoded voice empty".to_string(),
                                    user_id: Some(user_id.get().to_string()),
                                    ssrc: Some(*ssrc),
                                    sequence: None,
                                    timestamp: None,
                                    payload_len: Some(0),
                                    payload_offset: None,
                                    payload_end_pad: None,
                                    has_dave_marker: None,
                                    decoded_users: None,
                                    silent_users: None,
                                    audio_frames: Some(*self.active_frames.lock().await),
                                    decoded_samples: Some(0),
                                }).await;
                            }
                        }
                    }
                }
                if changed || snapshot_changed {
                    self.publish_stream_snapshot(false).await;
                }
            }
            EventContext::ClientDisconnect(disconnect) => {
                let mut ssrc_map = self.ssrc_to_user.lock().await;
                let removed_ssrcs = ssrc_map
                    .iter()
                    .filter_map(|(ssrc, user_id)| (user_id.get() == disconnect.user_id.0).then_some(*ssrc))
                    .collect::<Vec<_>>();
                ssrc_map.retain(|_, user_id| user_id.get() != disconnect.user_id.0);
                drop(ssrc_map);
                self.speaking_state.lock().await.remove(&Id::new(disconnect.user_id.0));
                if !removed_ssrcs.is_empty() {
                    let mut buffers = self.audio_buffers.lock().await;
                    for ssrc in removed_ssrcs {
                        buffers.remove(&ssrc);
                    }
                }
                let _ = self.activity_tx.send(AppEvent::Custom {
                    name: "voice_disconnect".to_string(),
                    payload: serde_json::json!({
                        "guild_id": self.guild_id.get().to_string(),
                        "user_id": disconnect.user_id.0.to_string(),
                    }),
                });
                self.publish_stream_snapshot(true).await;
            }
            EventContext::DriverConnect(connect) => {
                self.emit_signal_trace(VoiceSignalTrace {
                    guild_id: connect.guild_id.0.get().to_string(),
                    stage: "driver_connect".to_string(),
                    message: "voice driver connected".to_string(),
                    user_id: None,
                    channel_id: connect.channel_id.map(|id| id.0.get().to_string()),
                    ssrc: Some(connect.ssrc),
                }).await;
                let _ = self.activity_tx.send(AppEvent::Custom {
                    name: "voice_driver_connect".to_string(),
                    payload: serde_json::json!({
                        "guild_id": connect.guild_id.0.get().to_string(),
                        "channel_id": connect.channel_id.map(|id| id.0.get().to_string()),
                        "server": connect.server,
                        "session_id": connect.session_id,
                        "ssrc": connect.ssrc,
                    }),
                });
            }
            EventContext::DriverReconnect(connect) => {
                self.emit_signal_trace(VoiceSignalTrace {
                    guild_id: connect.guild_id.0.get().to_string(),
                    stage: "driver_reconnect".to_string(),
                    message: "voice driver reconnected".to_string(),
                    user_id: None,
                    channel_id: connect.channel_id.map(|id| id.0.get().to_string()),
                    ssrc: Some(connect.ssrc),
                }).await;
                let _ = self.activity_tx.send(AppEvent::Custom {
                    name: "voice_driver_reconnect".to_string(),
                    payload: serde_json::json!({
                        "guild_id": connect.guild_id.0.get().to_string(),
                        "channel_id": connect.channel_id.map(|id| id.0.get().to_string()),
                        "server": connect.server,
                        "session_id": connect.session_id,
                        "ssrc": connect.ssrc,
                    }),
                });
            }
            EventContext::DriverDisconnect(disconnect) => {
                self.emit_signal_trace(VoiceSignalTrace {
                    guild_id: disconnect.guild_id.0.get().to_string(),
                    stage: "driver_disconnect".to_string(),
                    message: format!("driver disconnect: {:?}", disconnect.kind),
                    user_id: None,
                    channel_id: disconnect.channel_id.map(|id| id.0.get().to_string()),
                    ssrc: None,
                }).await;
                let _ = self.activity_tx.send(AppEvent::Custom {
                    name: "voice_driver_disconnect".to_string(),
                    payload: serde_json::json!({
                        "guild_id": disconnect.guild_id.0.get().to_string(),
                        "channel_id": disconnect.channel_id.map(|id| id.0.get().to_string()),
                        "session_id": disconnect.session_id,
                        "kind": format!("{:?}", disconnect.kind),
                        "reason": disconnect.reason.map(|reason| format!("{:?}", reason)),
                    }),
                });
            }
            _ => {}
        }

        None
    }
}

pub async fn join_voice(state: &AppState, guild_id: Id<GuildMarker>, channel_id: Id<ChannelMarker>) -> JoinResult<VoiceSession> {
    state.voice_engine.join(state, guild_id, channel_id).await
}

pub fn classify_join_error(err: &JoinError) -> JoinFailureKind {
    if dave::is_dave_required_join_error(err) {
        return JoinFailureKind::RequiresDave;
    }

    if err.should_reconnect_driver() {
        JoinFailureKind::DriverConnection
    } else {
        JoinFailureKind::Other
    }
}

pub fn join_error_causes(err: &JoinError) -> Vec<String> {
    let mut causes = vec![err.to_string()];
    let mut current: &dyn StdError = err;

    while let Some(source) = current.source() {
        let text = source.to_string();
        if causes.last().is_none_or(|last| last != &text) {
            causes.push(text);
        }
        current = source;
    }

    causes
}

pub fn describe_join_error(err: &JoinError) -> String {
    let causes = join_error_causes(err);
    if causes.len() <= 1 {
        return causes.into_iter().next().unwrap_or_else(|| "failed to join voice channel".to_string());
    }

    format!("{}; cause: {}", causes[0], causes[1..].join(" -> "))
}

pub async fn leave_voice(state: &AppState, guild_id: Id<GuildMarker>) -> Result<()> {
    state.voice_engine.leave(state, guild_id).await
}

pub fn describe_voice_session(session: Option<VoiceSession>) -> String {
    match session {
        Some(session) => format!("connected to {:?}", session.channel_id),
        None => "not connected".to_string(),
    }
}
