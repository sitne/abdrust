use crate::{
    dave,
    state::{AppEvent, AppState, VoiceCapabilities, VoiceJoinState, VoiceSession, VoiceSignalTrace, VoiceUserMeta},
    voice,
    voice::{
        gateway::{VoiceGateway, VoiceEvent, DisconnectReason},
        session::DaveyVoiceSession,
        udp::{VoiceUdpSocket, IpDiscovery},
        rtp::RtpHeader,
    },
};
use anyhow::Result;
use async_trait::async_trait;
use crate::voice::udp::OpusEncoder;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, Mutex, watch};
use tracing::{debug, error, info, warn};
use twilight_model::{
    gateway::payload::{incoming::{VoiceServerUpdate, VoiceStateUpdate}, outgoing::update_voice_state::UpdateVoiceState},
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
    ) -> Result<VoiceSession>;
    async fn leave(&self, state: &AppState, guild_id: Id<GuildMarker>) -> Result<()>;
}

/// Audio data to send over voice
#[derive(Clone)]
pub enum AudioData {
    /// Raw Opus packet (already encoded)
    Opus(Vec<u8>),
    /// PCM samples (will be encoded to Opus)
    Pcm(Vec<i16>),
    /// Silence frame
    Silence,
}

/// Active voice connection for a guild
struct ActiveVoiceConnection {
    guild_id: Id<GuildMarker>,
    channel_id: Id<ChannelMarker>,
    session_id: String,
    token: String,
    endpoint: String,
    user_id: Id<twilight_model::id::marker::UserMarker>,
    shutdown_tx: Option<watch::Sender<bool>>,
    audio_tx: Option<broadcast::Sender<AudioData>>,
}

/// Shared state for voice connections
struct VoiceConnectionState {
    connections: HashMap<Id<GuildMarker>, ActiveVoiceConnection>,
}

pub struct DaveyVoiceEngine {
    bot_user_id: Id<twilight_model::id::marker::UserMarker>,
    connections: Arc<Mutex<VoiceConnectionState>>,
}

impl DaveyVoiceEngine {
    pub fn new(bot_user_id: Id<twilight_model::id::marker::UserMarker>) -> Self {
        Self {
            bot_user_id,
            connections: Arc::new(Mutex::new(VoiceConnectionState {
                connections: HashMap::new(),
            })),
        }
    }

    /// Send audio data to a voice channel
    pub async fn send_audio(&self, guild_id: Id<GuildMarker>, audio: AudioData) -> Result<()> {
        let connections = self.connections.lock().await;
        if let Some(conn) = connections.connections.get(&guild_id) {
            if let Some(tx) = &conn.audio_tx {
                let _ = tx.send(audio);
                return Ok(());
            }
        }
        anyhow::bail!("no active voice connection for guild {}", guild_id.get())
    }

    /// Wait for voice events (session_id from VoiceStateUpdate, token+endpoint from VoiceServerUpdate)
    async fn wait_for_voice_events(
        &self,
        state: &AppState,
        guild_id: Id<GuildMarker>,
    ) -> Result<(String, String, String)> {
        let mut rx = state.event_tx.subscribe();
        let guild_id_str = guild_id.get().to_string();

        let mut session_id: Option<String> = None;
        let mut token: Option<String> = None;
        let mut endpoint: Option<String> = None;

        // Check if we already have pending voice info from VoiceServerUpdate
        if let Some(info) = state.take_pending_voice_info(guild_id).await {
            token = Some(info.token);
            endpoint = Some(info.endpoint);
        }

        let timeout = tokio::time::sleep(Duration::from_secs(15));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    anyhow::bail!("timed out waiting for voice events (session_id={:?}, token={:?}, endpoint={:?})", session_id, token, endpoint);
                }
                event = rx.recv() => {
                    match event {
                        Ok(AppEvent::VoiceSessionReady { guild_id: gid, session_id: sid, token: t, endpoint: e }) => {
                            if gid == guild_id_str {
                                session_id = Some(sid);
                                token = Some(t);
                                endpoint = Some(e);
                            }
                        }
                        Ok(_) => continue,
                        Err(_) => anyhow::bail!("event channel closed"),
                    }
                }
            }

            if session_id.is_some() && token.is_some() && endpoint.is_some() {
                break;
            }
        }

        Ok((
            session_id.unwrap(),
            token.unwrap(),
            endpoint.unwrap(),
        ))
    }

    /// Run the voice gateway event loop
    async fn run_voice_loop_inner(
        mut gateway: VoiceGateway,
        state: AppState,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
        user_id: u64,
        mut shutdown_rx: watch::Receiver<bool>,
        mut audio_rx: broadcast::Receiver<AudioData>,
    ) -> DisconnectReason {
        info!("Starting voice gateway loop for guild {}", guild_id.get());

        // Send Identify immediately after connecting
        if let Err(e) = gateway.identify().await {
            error!("Failed to send identify: {}", e);
            return DisconnectReason::GatewayError(format!("identify failed: {}", e));
        }
        info!("Sent voice identify");

        // Heartbeat timer
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // UDP socket for receiving audio
        let udp_socket: Arc<Mutex<Option<VoiceUdpSocket>>> = Arc::new(Mutex::new(None));
        let dave_session: Arc<Mutex<Option<DaveyVoiceSession>>> = Arc::new(Mutex::new(None));

        // UDP receive buffer
        let mut udp_buf = vec![0u8; 4096];

        // Opus encoder for sending audio
        let mut opus_encoder = OpusEncoder::new().ok();

        // Map SSRC -> user_id for decryption
        let ssrc_to_user: Arc<Mutex<HashMap<u32, u64>>> = Arc::new(Mutex::new(HashMap::new()));

        loop {
            tokio::select! {
                Ok(()) = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Voice loop shutting down for guild {}", guild_id.get());
                        return DisconnectReason::NormalClose;
                    }
                }
                _ = heartbeat_interval.tick() => {
                    if let Err(e) = gateway.heartbeat().await {
                        warn!("Heartbeat failed: {}", e);
                    }
                }
                // UDP receive (non-blocking poll)
                _ = async {
                    let sock = udp_socket.lock().await;
                    if let Some(udp) = sock.as_ref() {
                        match udp.recv(&mut udp_buf) {
                            Ok(Some(len)) => {
                                let data = udp_buf[..len].to_vec();
                                // Parse RTP header
                                match RtpHeader::parse(&data) {
                                    Ok((header, payload_offset)) => {
                                        let payload = &data[payload_offset..];
                                        debug!("UDP audio: SSRC={}, seq={}, payload={} bytes", header.ssrc, header.sequence, payload.len());

                                        // Look up user_id for this SSRC
                                        let ssrc_map = ssrc_to_user.lock().await;
                                        if let Some(&sender_user_id) = ssrc_map.get(&header.ssrc) {
                                            drop(ssrc_map);

                                            // Try to decrypt with DAVE session
                                            let mut ds = dave_session.lock().await;
                                            if let Some(session) = ds.as_mut() {
                                                if session.is_ready() {
                                                    match session.decrypt(sender_user_id, payload) {
                                                        Ok(decrypted) => {
                                                            debug!("Decrypted audio: {} bytes for user {}", decrypted.len(), sender_user_id);
                                                            let _ = state.event_tx.send(AppEvent::VoiceAudioFrame {
                                                                guild_id: guild_id.get().to_string(),
                                                                user_id: sender_user_id.to_string(),
                                                                ssrc: header.ssrc,
                                                                samples: decrypted.len() / 2, // i16 samples
                                                            });
                                                        }
                                                        Err(e) => {
                                                            debug!("Failed to decrypt for user {}: {}", sender_user_id, e);
                                                        }
                                                    }
                                                } else {
                                                    // DAVE not ready yet, passthrough raw payload
                                                    debug!("DAVE not ready, passthrough {} bytes", payload.len());
                                                    let _ = state.event_tx.send(AppEvent::VoiceAudioFrame {
                                                        guild_id: guild_id.get().to_string(),
                                                        user_id: sender_user_id.to_string(),
                                                        ssrc: header.ssrc,
                                                        samples: payload.len(),
                                                    });
                                                }
                                            } else {
                                                // No DAVE session, passthrough
                                                debug!("No DAVE session, passthrough {} bytes", payload.len());
                                                let _ = state.event_tx.send(AppEvent::VoiceAudioFrame {
                                                    guild_id: guild_id.get().to_string(),
                                                    user_id: sender_user_id.to_string(),
                                                    ssrc: header.ssrc,
                                                    samples: payload.len(),
                                                });
                                            }
                                        } else {
                                            drop(ssrc_map);
                                            debug!("Unknown SSRC: {}", header.ssrc);
                                        }
                                    }
                                    Err(e) => {
                                        debug!("Failed to parse RTP header: {}", e);
                                    }
                                }
                            }
                            Ok(None) => {
                                // No data available
                            }
                            Err(e) => {
                                debug!("UDP recv error: {}", e);
                            }
                        }
                    }
                } => {}
                event = gateway.recv() => {
                    match event {
                        Ok(VoiceEvent::Ready { ssrc, ip, port, modes }) => {
                            info!("Voice Ready: SSRC={}, UDP={}:{}, modes={:?}", ssrc, ip, port, modes);

                            // Create a single UDP socket for discovery, receive, and send
                            let local_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
                            let remote_addr: SocketAddr = format!("{}:{}", ip, port).parse().unwrap();
                            let udp_socket_raw = match std::net::UdpSocket::bind(local_addr) {
                                Ok(s) => s,
                                Err(e) => {
                                    error!("Failed to bind UDP socket: {}", e);
                                    continue;
                                }
                            };
                            udp_socket_raw.set_nonblocking(true).ok();

                            // IP Discovery using the same socket
                            let discovery = IpDiscovery::new(
                                udp_socket_raw.try_clone().unwrap(),
                                remote_addr,
                                ssrc,
                            );
                            let (discovered_ip, discovered_port) = match discovery.discover() {
                                Ok(r) => r,
                                Err(e) => {
                                    error!("IP discovery failed: {}", e);
                                    continue;
                                }
                            };

                            // Select protocol
                            let mode = if modes.contains(&"aead_aes256_gcm_rtpsize".to_string()) {
                                "aead_aes256_gcm_rtpsize"
                            } else if modes.contains(&"aead_xchacha20_poly1305_rtpsize".to_string()) {
                                "aead_xchacha20_poly1305_rtpsize"
                            } else {
                                warn!("No preferred encryption mode available, using first available");
                                modes.first().map(|s| s.as_str()).unwrap_or("aead_xchacha20_poly1305_rtpsize")
                            };

                            if let Err(e) = gateway.select_protocol(
                                discovered_ip,
                                discovered_port,
                                mode.to_string(),
                            ).await {
                                error!("Select protocol failed: {}", e);
                                continue;
                            }

                            // Convert the raw socket to VoiceUdpSocket for receive/send
                            // We need to wrap it properly - VoiceUdpSocket::from_raw
                            match VoiceUdpSocket::from_raw(
                                udp_socket_raw,
                                remote_addr,
                                ssrc,
                                [0u8; 32], // secret_key will be set later from SessionDescription
                                mode.to_string(),
                            ) {
                                Ok(udp) => {
                                    *udp_socket.lock().await = Some(udp);
                                    debug!("Created VoiceUdpSocket from discovery socket");
                                }
                                Err(e) => {
                                    warn!("Failed to create VoiceUdpSocket: {}", e);
                                }
                            }
                        }
                        Ok(VoiceEvent::SessionDescription { mode, secret_key, dave_protocol_version }) => {
                            info!(
                                "Session Description: mode={}, dave_version={}",
                                mode, dave_protocol_version
                            );

                            // Create DAVE session if protocol version > 0
                            if dave_protocol_version > 0 {
                                match DaveyVoiceSession::new(
                                    user_id,
                                    channel_id.get(),
                                    dave_protocol_version,
                                ) {
                                    Ok(session) => {
                                        info!("Created DAVE session for protocol v{}", dave_protocol_version);
                                        *dave_session.lock().await = Some(session);
                                    }
                                    Err(e) => {
                                        warn!("Failed to create DAVE session: {}", e);
                                    }
                                }
                            }

                            // Update secret_key in the existing UDP socket
                            {
                                let mut sock = udp_socket.lock().await;
                                if let Some(udp) = sock.as_mut() {
                                    udp.set_secret_key(secret_key);
                                }
                            }

                            // Mark as ready
                            state.set_voice_join_state(
                                guild_id,
                                VoiceJoinState::Joined {
                                    guild_id: guild_id.get().to_string(),
                                    user_id: user_id.to_string(),
                                    channel_id: channel_id.get().to_string(),
                                    message: format!("joined (DAVE v{})", dave_protocol_version),
                                },
                            ).await;

                            let _ = state.event_tx.send(AppEvent::VoiceJoinResult {
                                guild_id: guild_id.get().to_string(),
                                user_id: user_id.to_string(),
                                ok: true,
                                message: format!("joined (DAVE v{})", dave_protocol_version),
                            });
                        }
                        Ok(VoiceEvent::Hello { heartbeat_interval: hi }) => {
                            heartbeat_interval = tokio::time::interval(Duration::from_millis(hi));
                            heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        }
                        Ok(VoiceEvent::DaveMlsExternalSender { external_sender_package }) => {
                            debug!("Received DAVE MLS External Sender: {} bytes", external_sender_package.len());
                            let mut ds = dave_session.lock().await;
                            if let Some(session) = ds.as_mut() {
                                if let Err(e) = session.set_external_sender(&external_sender_package) {
                                    warn!("Failed to set external sender: {}", e);
                                } else {
                                    // Send key package
                                    match session.create_key_package() {
                                        Ok(kp) => {
                                            if let Err(e) = gateway.send_key_package(&kp).await {
                                                warn!("Failed to send key package: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to create key package: {}", e);
                                        }
                                    }
                                }
                            } else {
                                warn!("Received external sender but no DAVE session");
                            }
                        }
                        Ok(VoiceEvent::DaveMlsProposals { operation_type, proposals }) => {
                            debug!("Received DAVE MLS Proposals: op_type={}, {} bytes", operation_type, proposals.len());
                            let mut ds = dave_session.lock().await;
                            if let Some(session) = ds.as_mut() {
                                match session.process_proposals(operation_type, &proposals) {
                                    Ok(Some((commit, welcome))) => {
                                        if let Err(e) = gateway.send_commit_welcome(&commit, welcome.as_deref()).await {
                                            warn!("Failed to send commit welcome: {}", e);
                                        }
                                    }
                                    Ok(None) => {
                                        debug!("No commit needed for proposals");
                                    }
                                    Err(e) => {
                                        warn!("Failed to process proposals: {}", e);
                                        if let Err(e) = gateway.send_invalid_commit_welcome(&e.to_string()).await {
                                            warn!("Failed to send invalid commit welcome: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Ok(VoiceEvent::DaveMlsAnnounceCommit { commit }) => {
                            debug!("Received DAVE MLS Announce Commit: {} bytes", commit.len());
                            let mut ds = dave_session.lock().await;
                            if let Some(session) = ds.as_mut() {
                                if let Err(e) = session.process_commit(&commit) {
                                    warn!("Failed to process commit: {}", e);
                                    if let Err(e) = gateway.send_invalid_commit_welcome(&e.to_string()).await {
                                        warn!("Failed to send invalid commit welcome: {}", e);
                                    }
                                } else if session.is_ready() {
                                    info!("DAVE session is now ready for encryption/decryption");
                                    let privacy_code = session.voice_privacy_code().unwrap_or("unknown").to_string();
                                    state.record_voice_signal_trace(VoiceSignalTrace {
                                        guild_id: guild_id.get().to_string(),
                                        stage: "dave_ready".to_string(),
                                        message: format!("DAVE session ready, privacy_code={}", privacy_code),
                                        user_id: None,
                                        channel_id: Some(channel_id.get().to_string()),
                                        ssrc: gateway.ssrc(),
                                    }).await;
                                }
                            }
                        }
                        Ok(VoiceEvent::DaveMlsWelcome { welcome }) => {
                            debug!("Received DAVE MLS Welcome: {} bytes", welcome.len());
                            let mut ds = dave_session.lock().await;
                            if let Some(session) = ds.as_mut() {
                                if let Err(e) = session.process_welcome(&welcome) {
                                    warn!("Failed to process welcome: {}", e);
                                    if let Err(e) = gateway.send_invalid_commit_welcome(&e.to_string()).await {
                                        warn!("Failed to send invalid commit welcome: {}", e);
                                    }
                                } else if session.is_ready() {
                                    info!("DAVE session ready after welcome");
                                    let privacy_code = session.voice_privacy_code().unwrap_or("unknown").to_string();
                                    state.record_voice_signal_trace(VoiceSignalTrace {
                                        guild_id: guild_id.get().to_string(),
                                        stage: "dave_ready_welcome".to_string(),
                                        message: format!("DAVE session ready via welcome, privacy_code={}", privacy_code),
                                        user_id: None,
                                        channel_id: Some(channel_id.get().to_string()),
                                        ssrc: gateway.ssrc(),
                                    }).await;
                                }
                            }
                        }
                        Ok(VoiceEvent::DavePrepareEpoch { epoch, transition_id }) => {
                            info!("DAVE Prepare Epoch: epoch={}, transition={}", epoch, transition_id);
                            // Send transition ready
                            if let Err(e) = gateway.send_transition_ready(&transition_id).await {
                                warn!("Failed to send transition ready: {}", e);
                            }
                        }
                        Ok(VoiceEvent::AudioFrame { data, opcode }) => {
                            // Audio frame from WebSocket (shouldn't normally happen, audio goes through UDP)
                            debug!("Audio frame via WS: opcode={}, {} bytes", opcode, data.len());
                        }
                        Ok(VoiceEvent::SpeakingUpdate { ssrc, user_id: speaking_user_id, speaking }) => {
                            debug!("Speaking update: SSRC={}, user={}, speaking={}", ssrc, speaking_user_id, speaking);
                            // Update SSRC -> user_id mapping
                            {
                                let mut map = ssrc_to_user.lock().await;
                                if speaking {
                                    map.insert(ssrc, speaking_user_id);
                                } else {
                                    map.retain(|_, &mut uid| uid != speaking_user_id);
                                }
                            }
                            // Forward to activity
                            let _ = state.event_tx.send(AppEvent::VoiceSpeaking {
                                guild_id: guild_id.get().to_string(),
                                user_id: speaking_user_id.to_string(),
                                channel_id: Some(channel_id.get().to_string()),
                                user_name: None,
                                display_name: None,
                                avatar_url: None,
                                ssrc,
                                speaking,
                            });
                        }
                        Ok(VoiceEvent::Closed) => {
                            warn!("Voice gateway closed");
                            return DisconnectReason::NormalClose;
                        }
                        Ok(VoiceEvent::HeartbeatAck) | Ok(VoiceEvent::Pong) => {
                            // Normal
                        }
                        Ok(VoiceEvent::ClientDisconnect) => {
                            debug!("Client disconnected from voice");
                        }
                        Ok(VoiceEvent::ClientsConnect) => {
                            debug!("Clients connected to voice");
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Voice gateway error: {}", e);
                            return DisconnectReason::GatewayError(e.to_string());
                        }
                    }
                }
                // Audio send
                audio = audio_rx.recv() => {
                    match audio {
                        Ok(audio_data) => {
                            let mut sock = udp_socket.lock().await;
                            if let Some(udp) = sock.as_mut() {
                                match audio_data {
                                    AudioData::Silence => {
                                        let silence = OpusEncoder::silence_frame();
                                        let mut ds = dave_session.lock().await;
                                        if let Some(session) = ds.as_mut() {
                                            if session.is_ready() {
                                                match session.encrypt_opus(&silence) {
                                                    Ok(encrypted) => {
                                                        if let Err(e) = udp.send_dave_encrypted(&encrypted) {
                                                            warn!("Failed to send silence: {}", e);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to encrypt silence: {}", e);
                                                    }
                                                }
                                            } else if let Err(e) = udp.send(&silence) {
                                                warn!("Failed to send silence: {}", e);
                                            }
                                        } else if let Err(e) = udp.send(&silence) {
                                            warn!("Failed to send silence: {}", e);
                                        }
                                    }
                                    AudioData::Opus(opus_data) => {
                                        let mut ds = dave_session.lock().await;
                                        if let Some(session) = ds.as_mut() {
                                            if session.is_ready() {
                                                match session.encrypt_opus(&opus_data) {
                                                    Ok(encrypted) => {
                                                        if let Err(e) = udp.send_dave_encrypted(&encrypted) {
                                                            warn!("Failed to send encrypted opus: {}", e);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to encrypt opus: {}", e);
                                                    }
                                                }
                                            } else if let Err(e) = udp.send(&opus_data) {
                                                warn!("Failed to send opus: {}", e);
                                            }
                                        } else if let Err(e) = udp.send(&opus_data) {
                                            warn!("Failed to send opus: {}", e);
                                        }
                                    }
                                    AudioData::Pcm(pcm_data) => {
                                        if let Some(encoder) = opus_encoder.as_mut() {
                                            match encoder.encode(&pcm_data) {
                                                Ok(opus_data) => {
                                                    let mut ds = dave_session.lock().await;
                                                    if let Some(session) = ds.as_mut() {
                                                        if session.is_ready() {
                                                            match session.encrypt_opus(&opus_data) {
                                                                Ok(encrypted) => {
                                                                    if let Err(e) = udp.send_dave_encrypted(&encrypted) {
                                                                        warn!("Failed to send encrypted pcm: {}", e);
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    warn!("Failed to encrypt pcm: {}", e);
                                                                }
                                                            }
                                                        } else if let Err(e) = udp.send(&opus_data) {
                                                            warn!("Failed to send opus: {}", e);
                                                        }
                                                    } else if let Err(e) = udp.send(&opus_data) {
                                                        warn!("Failed to send opus: {}", e);
                                                    }
                                                }
                                                Err(e) => {
                                                    warn!("Failed to encode PCM to Opus: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Audio channel lagged, dropped {} messages", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!("Audio channel closed");
                        }
                    }
                }
            }
        }
    }

    /// Run voice loop with automatic reconnection
    async fn run_voice_loop_with_reconnect(
        endpoint: String,
        session_id: String,
        token: String,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
        bot_user_id: u64,
        state: AppState,
        audio_rx: broadcast::Receiver<AudioData>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let mut reconnect_attempt = 0;
        let max_reconnect_delay = Duration::from_secs(60);

        loop {
            // Check for shutdown
            if *shutdown_rx.borrow() {
                info!("Shutdown requested, not reconnecting");
                return;
            }

            // Calculate reconnect delay with exponential backoff
            let delay = std::cmp::min(
                Duration::from_secs(2_u64.saturating_pow(reconnect_attempt.min(5))),
                max_reconnect_delay,
            );

            if reconnect_attempt > 0 {
                info!(
                    "Reconnecting to voice gateway (attempt {}, delay={:?})",
                    reconnect_attempt, delay
                );
                state.record_voice_signal_trace(VoiceSignalTrace {
                    guild_id: guild_id.get().to_string(),
                    stage: "reconnect_attempt".to_string(),
                    message: format!("attempt={}, delay={:?}", reconnect_attempt, delay),
                    user_id: None,
                    channel_id: Some(channel_id.get().to_string()),
                    ssrc: None,
                }).await;

                // Wait for delay or shutdown
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("Shutdown during reconnect delay");
                            return;
                        }
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }

            // Reconnect to voice gateway
            match VoiceGateway::connect(
                &endpoint,
                session_id.clone(),
                token.clone(),
                guild_id.get().to_string(),
                bot_user_id.to_string(),
                dave::MAX_DAVE_PROTOCOL_VERSION,
            ).await {
                Ok(gateway) => {
                    info!("Reconnected to voice gateway");
                    state.record_voice_signal_trace(VoiceSignalTrace {
                        guild_id: guild_id.get().to_string(),
                        stage: "reconnect_success".to_string(),
                        message: format!("attempt={}", reconnect_attempt),
                        user_id: None,
                        channel_id: Some(channel_id.get().to_string()),
                        ssrc: None,
                    }).await;

                    // Reset reconnect counter on successful connection
                    reconnect_attempt = 0;

                    // Clone shutdown receiver for the inner loop
                    let inner_shutdown = shutdown_rx.clone();
                    let inner_audio = audio_rx.resubscribe();

                    // Run the voice loop
                    let reason = Self::run_voice_loop_inner(
                        gateway,
                        state.clone(),
                        guild_id,
                        channel_id,
                        bot_user_id,
                        inner_shutdown,
                        inner_audio,
                    ).await;

                    match reason {
                        DisconnectReason::NormalClose => {
                            info!("Voice loop ended normally");
                            return;
                        }
                        DisconnectReason::Fatal(msg) => {
                            error!("Fatal voice error: {}", msg);
                            state.record_voice_signal_trace(VoiceSignalTrace {
                                guild_id: guild_id.get().to_string(),
                                stage: "reconnect_fatal".to_string(),
                                message: msg,
                                user_id: None,
                                channel_id: Some(channel_id.get().to_string()),
                                ssrc: None,
                            }).await;
                            return;
                        }
                        _ => {
                            warn!("Voice loop ended with {:?}, will reconnect", reason);
                            reconnect_attempt += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to reconnect to voice gateway: {}", e);
                    state.record_voice_signal_trace(VoiceSignalTrace {
                        guild_id: guild_id.get().to_string(),
                        stage: "reconnect_failed".to_string(),
                        message: format!("attempt={}, error={}", reconnect_attempt, e),
                        user_id: None,
                        channel_id: Some(channel_id.get().to_string()),
                        ssrc: None,
                    }).await;
                    reconnect_attempt += 1;
                }
            }
        }
    }
}

#[async_trait]
impl VoiceEngine for DaveyVoiceEngine {
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

            if event.channel_id.is_some() {
                state.set_user_voice_state(guild_id, event.user_id, event.channel_id.unwrap()).await;
                // Store bot's session_id for voice gateway connection
                if event.user_id == self.bot_user_id {
                    state.bot.bot_pending_session_id.lock().await.insert(guild_id, event.session_id.clone());
                }
            } else {
                state.remove_user_voice_state(guild_id, event.user_id).await;
                state.bot.bot_pending_session_id.lock().await.remove(&guild_id);
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

        // Store voice info and emit VoiceSessionReady if we have a session_id
        if let Some(endpoint) = &event.endpoint {
            // Get actual session_id from bot's pending session (set by VoiceStateUpdate handler)
            let session_id = state.bot.bot_pending_session_id.lock().await.get(&event.guild_id).cloned();

            if let Some(sid) = session_id {
                state.store_pending_voice_info(
                    event.guild_id,
                    event.token.clone(),
                    endpoint.clone(),
                ).await;

                // Emit VoiceSessionReady so the join flow can proceed
                let _ = state.event_tx.send(AppEvent::VoiceSessionReady {
                    guild_id: event.guild_id.get().to_string(),
                    session_id: sid,
                    token: event.token,
                    endpoint: endpoint.clone(),
                });
            }
        }
    }

    async fn join(
        &self,
        state: &AppState,
        guild_id: Id<GuildMarker>,
        channel_id: Id<ChannelMarker>,
    ) -> Result<VoiceSession> {
        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "join_start".to_string(),
                message: "joining voice channel (DAVE)".to_string(),
                user_id: None,
                channel_id: Some(channel_id.get().to_string()),
                ssrc: None,
            })
            .await;

        // Send Voice State Update to Discord (Gateway Opcode 4)
        // This tells Discord we want to join the voice channel
        let voice_state_update = UpdateVoiceState::new(guild_id, Some(channel_id), false, false);
        let _ = state.bot.gateway.command(&voice_state_update);
        info!(
            guild_id = %guild_id.get(),
            channel_id = %channel_id.get(),
            "Sent Voice State Update to Discord"
        );

        // Wait for voice events (session_id from VoiceStateUpdate, token+endpoint from VoiceServerUpdate)
        let (session_id, token, endpoint) = self.wait_for_voice_events(state, guild_id).await?;

        info!(
            "Voice info received: session_id={}, endpoint={}, guild={}",
            session_id, endpoint, guild_id.get()
        );

        // Create shutdown channel (watch for cloneable shutdown signal)
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Create audio send channel (broadcast for multi-subscriber on reconnect)
        let (audio_tx, audio_rx) = broadcast::channel::<AudioData>(64);

        // Clone values for the spawn
        let session_id_clone = session_id.clone();
        let token_clone = token.clone();
        let endpoint_clone = endpoint.clone();

        // Store connection
        {
            let mut connections = self.connections.lock().await;
            connections.connections.insert(
                guild_id,
                ActiveVoiceConnection {
                    guild_id,
                    channel_id,
                    session_id,
                    token,
                    endpoint,
                    user_id: self.bot_user_id,
                    shutdown_tx: Some(shutdown_tx),
                    audio_tx: Some(audio_tx),
                },
            );
        }

        // Set joining state
        state.set_voice_join_state(
            guild_id,
            VoiceJoinState::Joining {
                guild_id: guild_id.get().to_string(),
                user_id: self.bot_user_id.get().to_string(),
                channel_id: channel_id.get().to_string(),
                message: "connecting to voice gateway".to_string(),
            },
        ).await;

        // Start voice loop with reconnection
        let state_clone = state.clone();
        let bot_user_id = self.bot_user_id;
        tokio::spawn(async move {
            DaveyVoiceEngine::run_voice_loop_with_reconnect(
                endpoint_clone,
                session_id_clone,
                token_clone,
                guild_id,
                channel_id,
                bot_user_id.get(),
                state_clone,
                audio_rx,
                shutdown_rx,
            )
            .await;
        });

        let session = VoiceSession {
            guild_id: guild_id.get().to_string(),
            channel_id: Some(channel_id.get().to_string()),
        };
        state.set_voice_session(guild_id, session.clone()).await;

        state
            .record_voice_signal_trace(VoiceSignalTrace {
                guild_id: guild_id.get().to_string(),
                stage: "join_gateway_connected".to_string(),
                message: "voice gateway connected, DAVE handshake in progress".to_string(),
                user_id: None,
                channel_id: Some(channel_id.get().to_string()),
                ssrc: None,
            })
            .await;

        Ok(session)
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

        // Send Voice State Update to Discord to actually leave the channel
        let leave_state = UpdateVoiceState::new(guild_id, None::<Id<ChannelMarker>>, false, false);
        let _ = state.bot.gateway.command(&leave_state);
        info!(guild_id = %guild_id.get(), "Sent Voice State Update to leave voice channel");

        // Shutdown voice loop
        {
            let mut connections = self.connections.lock().await;
            if let Some(conn) = connections.connections.remove(&guild_id) {
                if let Some(tx) = conn.shutdown_tx {
                    let _ = tx.send(true);
                }
            }
        }

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
