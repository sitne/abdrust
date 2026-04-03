use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_websockets::{ClientBuilder, Limits, Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, warn};

use crate::voice::session::DaveyVoiceSession;

/// Voice Gateway opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VoiceOpcode {
    Identify = 0,
    SelectProtocol = 1,
    Ready = 2,
    Heartbeat = 3,
    SessionDescription = 4,
    Speaking = 5,
    HeartbeatAck = 6,
    Resume = 7,
    Hello = 8,
    Resumed = 9,
    ClientsConnect = 11,
    ClientDisconnect = 13,
    DavePrepareTransition = 21,
    DaveExecuteTransition = 22,
    DaveTransitionReady = 23,
    DavePrepareEpoch = 24,
    DaveMlsExternalSender = 25,
    DaveMlsKeyPackage = 26,
    DaveMlsProposals = 27,
    DaveMlsCommitWelcome = 28,
    DaveMlsAnnounceCommitTransition = 29,
    DaveMlsWelcome = 30,
    DaveMlsInvalidCommitWelcome = 31,
}

/// Voice Gateway payloads
#[derive(Debug, Serialize, Deserialize)]
pub struct VoicePayload<T> {
    pub op: u8,
    pub d: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

/// Identify payload (opcode 0)
#[derive(Debug, Serialize)]
pub struct IdentifyPayload {
    pub server_id: String,
    pub user_id: String,
    pub session_id: String,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_dave_protocol_version: Option<u16>,
}

/// Select Protocol payload (opcode 1)
#[derive(Debug, Serialize)]
pub struct SelectProtocolPayload {
    pub protocol: String,
    pub data: SelectProtocolData,
}

#[derive(Debug, Serialize)]
pub struct SelectProtocolData {
    pub address: String,
    pub port: u16,
    pub mode: String,
}

/// Ready payload (opcode 2)
#[derive(Debug, Deserialize)]
pub struct ReadyPayload {
    pub ssrc: u32,
    pub ip: String,
    pub port: u16,
    pub modes: Vec<String>,
}

/// Heartbeat payload (opcode 3) - v8 format
#[derive(Debug, Serialize)]
pub struct HeartbeatPayload {
    pub t: u64,
    pub seq_ack: i64,
}

/// Session Description payload (opcode 4)
#[derive(Debug, Deserialize)]
pub struct SessionDescriptionPayload {
    pub mode: String,
    pub secret_key: Vec<u8>,
    pub dave_protocol_version: u16,
}

/// Hello payload (opcode 8)
#[derive(Debug, Deserialize)]
pub struct HelloPayload {
    pub heartbeat_interval: u64,
}

/// Speaking payload (opcode 5)
#[derive(Debug, Serialize)]
pub struct SpeakingPayload {
    pub speaking: u8,
    pub delay: u32,
    pub ssrc: u32,
}

/// DAVE MLS External Sender (opcode 25)
#[derive(Debug, Deserialize)]
pub struct DaveMlsExternalSenderPayload {
    #[serde(rename = "externalSenderPackage")]
    pub external_sender_package: Vec<u8>,
}

/// DAVE MLS Proposals (opcode 27)
#[derive(Debug, Deserialize)]
pub struct DaveMlsProposalsPayload {
    #[serde(rename = "operationType")]
    pub operation_type: u8,
    pub proposals: Vec<u8>,
}

/// DAVE MLS Announce Commit Transition (opcode 29)
#[derive(Debug, Deserialize)]
pub struct DaveMlsAnnounceCommitPayload {
    pub commit: Vec<u8>,
}

/// DAVE MLS Welcome (opcode 30)
#[derive(Debug, Deserialize)]
pub struct DaveMlsWelcomePayload {
    pub welcome: Vec<u8>,
}

/// DAVE Protocol Prepare Epoch (opcode 24)
#[derive(Debug, Deserialize)]
pub struct DavePrepareEpochPayload {
    pub epoch: u64,
    #[serde(rename = "transitionId")]
    pub transition_id: String,
}

/// DAVE Protocol Execute Transition (opcode 22)
#[derive(Debug, Deserialize)]
pub struct DaveExecuteTransitionPayload {
    #[serde(rename = "transitionId")]
    pub transition_id: String,
}

/// DAVE Protocol Prepare Transition (opcode 21)
#[derive(Debug, Deserialize)]
pub struct DavePrepareTransitionPayload {
    #[serde(rename = "transitionId")]
    pub transition_id: String,
}

/// Voice Gateway connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayState {
    Disconnected,
    Connecting,
    Identifying,
    Ready,
    SessionDescription,
    DAVEHandshake,
    Active,
}

/// Voice Gateway connection
pub struct VoiceGateway {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    state: GatewayState,
    heartbeat_interval: Duration,
    last_seq: i64,
    session_id: String,
    token: String,
    server_id: String,
    user_id: String,
    ssrc: Option<u32>,
    udp_ip: Option<String>,
    udp_port: Option<u16>,
    secret_key: Option<[u8; 32]>,
    encryption_mode: Option<String>,
    dave_protocol_version: u16,
    max_dave_protocol_version: u16,
    dave_session: Option<DaveyVoiceSession>,
}

impl VoiceGateway {
    /// Connect to voice gateway
    pub async fn connect(
        endpoint: &str,
        session_id: String,
        token: String,
        server_id: String,
        user_id: String,
        max_dave_version: u16,
    ) -> Result<Self> {
        let url = format!("wss://{}?v=8", endpoint);
        info!("Connecting to voice gateway: {}", url);

        let (ws, _response) = ClientBuilder::new()
            .uri(&url)
            .context("failed to parse voice gateway URL")?
            .limits(Limits::unlimited())
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect to voice gateway: {:?}", e))?;

        info!("Voice WebSocket connected");
        Ok(Self {
            ws,
            state: GatewayState::Connecting,
            heartbeat_interval: Duration::from_secs(0),
            last_seq: -1,
            session_id,
            token,
            server_id,
            user_id,
            ssrc: None,
            udp_ip: None,
            udp_port: None,
            secret_key: None,
            encryption_mode: None,
            dave_protocol_version: 0,
            max_dave_protocol_version: max_dave_version,
            dave_session: None,
        })
    }

    /// Send identify payload
    pub async fn identify(&mut self) -> Result<()> {
        let payload = IdentifyPayload {
            server_id: self.server_id.clone(),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            token: self.token.clone(),
            max_dave_protocol_version: if self.max_dave_protocol_version > 0 {
                Some(self.max_dave_protocol_version)
            } else {
                None
            },
        };

        self.send_json(0, &payload).await?;
        self.state = GatewayState::Identifying;
        info!("Sent voice identify");
        Ok(())
    }

    /// Send select protocol payload
    pub async fn select_protocol(
        &mut self,
        address: String,
        port: u16,
        mode: String,
    ) -> Result<()> {
        let mode_label = mode.clone();
        let payload = SelectProtocolPayload {
            protocol: "udp".to_string(),
            data: SelectProtocolData {
                address,
                port,
                mode,
            },
        };

        self.send_json(1, &payload).await?;
        info!("Sent select protocol: mode={}", mode_label);
        Ok(())
    }

    /// Send speaking payload
    pub async fn set_speaking(&mut self, speaking: bool) -> Result<()> {
        let ssrc = self.ssrc.context("no SSRC available")?;
        let payload = SpeakingPayload {
            speaking: if speaking { 1 } else { 0 },
            delay: 0,
            ssrc,
        };

        self.send_json(5, &payload).await?;
        Ok(())
    }

    /// Send DAVE transition ready (opcode 23) - binary message
    pub async fn send_transition_ready(&mut self, transition_id: u64) -> Result<()> {
        let mut buf = Vec::with_capacity(1 + 10);
        buf.push(23);
        encode_uleb128(&mut buf, transition_id);
        self.send_binary(buf).await?;
        debug!("Sent DAVE transition ready (transition_id={})", transition_id);
        Ok(())
    }

    /// Send DAVE MLS key package (opcode 26)
    pub async fn send_key_package(&mut self, key_package: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(1 + key_package.len());
        buf.push(26);
        buf.extend_from_slice(key_package);
        self.send_binary(buf).await?;
        debug!("Sent DAVE MLS key package ({} bytes)", key_package.len());
        Ok(())
    }

    /// Send DAVE MLS commit welcome (opcode 28) - binary message
    pub async fn send_commit_welcome(
        &mut self,
        commit: &[u8],
        welcome: Option<&[u8]>,
    ) -> Result<()> {
        // Binary format per DAVE whitepaper:
        // [opcode: u8][commit (variable)][welcome_length (ULEB128)][welcome (variable)]
        let mut buf = Vec::with_capacity(1 + commit.len() + 5 + welcome.map(|w| w.len()).unwrap_or(0));
        buf.push(28);
        buf.extend_from_slice(commit);
        if let Some(w) = welcome {
            encode_uleb128(&mut buf, w.len() as u64);
            buf.extend_from_slice(w);
        } else {
            encode_uleb128(&mut buf, 0);
        }
        self.send_binary(buf).await?;
        debug!("Sent DAVE MLS commit welcome (commit={} bytes)", commit.len());
        Ok(())
    }

    /// Send DAVE MLS invalid commit welcome (opcode 31) - binary message
    pub async fn send_invalid_commit_welcome(&mut self, transition_id: u64) -> Result<()> {
        let mut buf = Vec::with_capacity(1 + 10); // opcode + max ULEB128
        buf.push(31);
        encode_uleb128(&mut buf, transition_id);
        self.send_binary(buf).await?;
        debug!("Sent DAVE MLS invalid commit welcome (transition_id={})", transition_id);
        Ok(())
    }

    /// Send heartbeat
    pub async fn heartbeat(&mut self) -> Result<()> {
        let payload = HeartbeatPayload {
            t: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            seq_ack: self.last_seq,
        };

        self.send_json(3, &payload).await?;
        Ok(())
    }

    /// Receive and process next message
    pub async fn recv(&mut self) -> Result<VoiceEvent> {
        let msg = self
            .ws
            .next()
            .await
            .context("voice gateway connection closed")??;

        if msg.is_text() {
            let text = msg.as_text().context("failed to get text message")?;
            let value: serde_json::Value = serde_json::from_str(text)
                .context("failed to parse voice gateway message")?;

            let op = value
                .get("op")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u8;

            if let Some(seq) = value.get("seq").and_then(|v| v.as_i64()) {
                self.last_seq = seq;
            }

            self.handle_json_message(op, value).await
        } else if msg.is_binary() {
            let data = msg.into_payload();
            self.handle_binary_message(data.to_vec()).await
        } else if msg.is_close() {
            let payload = msg.into_payload();
            if !payload.is_empty() {
                let code = u16::from_be_bytes([payload[0], payload[1]]);
                let reason = String::from_utf8_lossy(&payload[2..]);
                warn!("Voice gateway closed: code={}, reason={}", code, reason);
            } else {
                info!("Voice gateway closed (no close frame)");
            }
            Ok(VoiceEvent::Closed)
        } else if msg.is_ping() {
            let data = msg.into_payload();
            self.ws.send(Message::pong(data)).await?;
            Ok(VoiceEvent::Ping)
        } else if msg.is_pong() {
            Ok(VoiceEvent::Pong)
        } else {
            Ok(VoiceEvent::Unknown)
        }
    }

    /// Handle JSON voice message
    async fn handle_json_message(&mut self, op: u8, value: serde_json::Value) -> Result<VoiceEvent> {
        let data = value.get("d").cloned().unwrap_or(serde_json::Value::Null);

        match op {
            2 => {
                let ready: ReadyPayload = serde_json::from_value(data)
                    .context("failed to parse Ready payload")?;
                self.ssrc = Some(ready.ssrc);
                self.udp_ip = Some(ready.ip.clone());
                self.udp_port = Some(ready.port);
                self.state = GatewayState::Ready;
                info!(
                    "Voice Ready: SSRC={}, UDP={}:{}",
                    ready.ssrc, ready.ip, ready.port
                );
                Ok(VoiceEvent::Ready {
                    ssrc: ready.ssrc,
                    ip: ready.ip,
                    port: ready.port,
                    modes: ready.modes,
                })
            }
            4 => {
                let desc: SessionDescriptionPayload = serde_json::from_value(data)
                    .context("failed to parse Session Description payload")?;
                self.encryption_mode = Some(desc.mode.clone());
                self.dave_protocol_version = desc.dave_protocol_version;

                let mut key = [0u8; 32];
                key.copy_from_slice(&desc.secret_key);
                self.secret_key = Some(key);

                self.state = GatewayState::SessionDescription;
                info!(
                    "Session Description: mode={}, dave_version={}",
                    desc.mode, desc.dave_protocol_version
                );
                Ok(VoiceEvent::SessionDescription {
                    mode: desc.mode,
                    secret_key: key,
                    dave_protocol_version: desc.dave_protocol_version,
                })
            }
            6 => Ok(VoiceEvent::HeartbeatAck),
            8 => {
                let hello: HelloPayload = serde_json::from_value(data)
                    .context("failed to parse Hello payload")?;
                self.heartbeat_interval = Duration::from_millis(hello.heartbeat_interval);
                info!("Hello: heartbeat_interval={}ms", hello.heartbeat_interval);
                Ok(VoiceEvent::Hello {
                    heartbeat_interval: hello.heartbeat_interval,
                })
            }
            9 => {
                info!("Resumed");
                Ok(VoiceEvent::Resumed)
            }
            21 => {
                let prep: DavePrepareTransitionPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE Prepare Transition")?;
                warn!("DAVE Prepare Transition: {}", prep.transition_id);
                Ok(VoiceEvent::DavePrepareTransition {
                    transition_id: prep.transition_id,
                })
            }
            22 => {
                let exec: DaveExecuteTransitionPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE Execute Transition")?;
                info!("DAVE Execute Transition: {}", exec.transition_id);
                Ok(VoiceEvent::DaveExecuteTransition {
                    transition_id: exec.transition_id,
                })
            }
            24 => {
                let prep: DavePrepareEpochPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE Prepare Epoch")?;
                info!(
                    "DAVE Prepare Epoch: epoch={}, transition={}",
                    prep.epoch, prep.transition_id
                );
                Ok(VoiceEvent::DavePrepareEpoch {
                    epoch: prep.epoch,
                    transition_id: prep.transition_id,
                })
            }
            25 => {
                let ext: DaveMlsExternalSenderPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE MLS External Sender")?;
                debug!(
                    "DAVE MLS External Sender: {} bytes",
                    ext.external_sender_package.len()
                );
                Ok(VoiceEvent::DaveMlsExternalSender {
                    external_sender_package: ext.external_sender_package,
                })
            }
            27 => {
                let props: DaveMlsProposalsPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE MLS Proposals")?;
                debug!(
                    "DAVE MLS Proposals: op_type={}, {} bytes",
                    props.operation_type,
                    props.proposals.len()
                );
                Ok(VoiceEvent::DaveMlsProposals {
                    operation_type: props.operation_type,
                    proposals: props.proposals,
                })
            }
            29 => {
                let commit: DaveMlsAnnounceCommitPayload = serde_json::from_value(data)
                    .context("failed to parse DAVE MLS Announce Commit")?;
                debug!("DAVE MLS Announce Commit (JSON): {} bytes", commit.commit.len());
                Ok(VoiceEvent::DaveMlsAnnounceCommit {
                    transition_id: 0,
                    commit: commit.commit,
                })
            }
            30 => {
                let welcome: DaveMlsWelcomePayload = serde_json::from_value(data)
                    .context("failed to parse DAVE MLS Welcome")?;
                debug!("DAVE MLS Welcome (JSON): {} bytes", welcome.welcome.len());
                Ok(VoiceEvent::DaveMlsWelcome {
                    transition_id: 0,
                    welcome: welcome.welcome,
                })
            }
            11 => Ok(VoiceEvent::ClientsConnect),
            13 => Ok(VoiceEvent::ClientDisconnect),
            5 => {
                if let (Some(ssrc), Some(user_id), Some(speaking)) = (
                    data.get("ssrc").and_then(|v| v.as_u64()),
                    data.get("user_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<u64>().ok()),
                    data.get("speaking").and_then(|v| v.as_u64()),
                ) {
                    Ok(VoiceEvent::SpeakingUpdate {
                        ssrc: ssrc as u32,
                        user_id,
                        speaking: speaking != 0,
                    })
                } else {
                    Ok(VoiceEvent::Unknown)
                }
            }
            _ => {
                warn!("Unknown voice opcode: {}", op);
                Ok(VoiceEvent::Unknown)
            }
        }
    }

    /// Handle binary voice message
    async fn handle_binary_message(&mut self, data: Vec<u8>) -> Result<VoiceEvent> {
        if data.is_empty() {
            return Ok(VoiceEvent::Unknown);
        }

        let (opcode, payload) = if data.len() >= 3 {
            let potential_opcode = data[2];
            if Self::is_dave_binary_opcode(potential_opcode) {
                let _seq = u16::from_be_bytes([data[0], data[1]]);
                (potential_opcode, &data[3..])
            } else {
                (data[0], &data[1..])
            }
        } else {
            (data[0], &data[1..])
        };

        match opcode {
            25 => {
                debug!("DAVE MLS External Sender (binary): {} bytes", payload.len());
                Ok(VoiceEvent::DaveMlsExternalSender {
                    external_sender_package: payload.to_vec(),
                })
            }
            27 => {
                if payload.is_empty() {
                    return Ok(VoiceEvent::Unknown);
                }
                let operation_type = payload[0];
                let proposals = payload[1..].to_vec();
                debug!(
                    "DAVE MLS Proposals (binary): op_type={}, {} bytes",
                    operation_type,
                    proposals.len()
                );
                Ok(VoiceEvent::DaveMlsProposals {
                    operation_type,
                    proposals,
                })
            }
            29 => {
                // Binary: [opcode][transition_id ULEB128][commit]
                if payload.is_empty() {
                    return Ok(VoiceEvent::Unknown);
                }
                let (transition_id, offset) = decode_uleb128(&payload);
                let commit = if offset < payload.len() {
                    payload[offset..].to_vec()
                } else {
                    Vec::new()
                };
                debug!("DAVE MLS Announce Commit: transition_id={}, commit={} bytes", transition_id, commit.len());
                Ok(VoiceEvent::DaveMlsAnnounceCommit { transition_id, commit })
            }
            30 => {
                // Binary: [opcode][transition_id ULEB128][welcome]
                if payload.is_empty() {
                    return Ok(VoiceEvent::Unknown);
                }
                let (transition_id, offset) = decode_uleb128(&payload);
                let welcome = if offset < payload.len() {
                    payload[offset..].to_vec()
                } else {
                    Vec::new()
                };
                debug!("DAVE MLS Welcome: transition_id={}, welcome={} bytes", transition_id, welcome.len());
                Ok(VoiceEvent::DaveMlsWelcome { transition_id, welcome })
            }
            _ => {
                Ok(VoiceEvent::AudioFrame {
                    data: payload.to_vec(),
                    opcode,
                })
            }
        }
    }

    fn is_dave_binary_opcode(opcode: u8) -> bool {
        matches!(opcode, 25 | 27 | 29 | 30)
    }

    /// Send JSON message
    async fn send_json<T: Serialize>(&mut self, op: u8, data: &T) -> Result<()> {
        let payload = serde_json::json!({
            "op": op,
            "d": data,
        });
        let text = serde_json::to_string(&payload)?;
        self.ws
            .send(Message::text(text))
            .await
            .context("failed to send voice message")?;
        Ok(())
    }

    /// Send binary message
    async fn send_binary(&mut self, data: Vec<u8>) -> Result<()> {
        self.ws
            .send(Message::binary(data))
            .await
            .context("failed to send binary voice message")?;
        Ok(())
    }

    /// Get heartbeat interval
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    /// Get SSRC
    pub fn ssrc(&self) -> Option<u32> {
        self.ssrc
    }

    /// Get UDP info
    pub fn udp_info(&self) -> Option<(String, u16)> {
        self.udp_ip.as_ref().and_then(|ip| {
            self.udp_port.map(|port| (ip.clone(), port))
        })
    }

    /// Get secret key
    pub fn secret_key(&self) -> Option<&[u8; 32]> {
        self.secret_key.as_ref()
    }

    /// Get encryption mode
    pub fn encryption_mode(&self) -> Option<&str> {
        self.encryption_mode.as_deref()
    }

    /// Get DAVE protocol version
    pub fn dave_protocol_version(&self) -> u16 {
        self.dave_protocol_version
    }

    /// Get current state
    pub fn state(&self) -> GatewayState {
        self.state
    }

    /// Get or create DAVE session
    pub fn dave_session_mut(&mut self) -> &mut Option<DaveyVoiceSession> {
        &mut self.dave_session
    }

    /// Set DAVE session
    pub fn set_dave_session(&mut self, session: DaveyVoiceSession) {
        self.dave_session = Some(session);
    }
}

/// Encode a u64 as ULEB128
fn encode_uleb128(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decode a ULEB128 value from bytes, returning (value, bytes_consumed)
fn decode_uleb128(data: &[u8]) -> (u64, usize) {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    let mut offset = 0;
    for &byte in data {
        value |= ((byte & 0x7F) as u64) << shift;
        offset += 1;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    (value, offset)
}

/// Disconnect reason for reconnection logic
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    NormalClose,
    ErrorClose(u16),
    GatewayError(String),
    SessionTimeout,
    Fatal(String),
}

/// Voice gateway events
pub enum VoiceEvent {
    Ready {
        ssrc: u32,
        ip: String,
        port: u16,
        modes: Vec<String>,
    },
    SessionDescription {
        mode: String,
        secret_key: [u8; 32],
        dave_protocol_version: u16,
    },
    Hello {
        heartbeat_interval: u64,
    },
    HeartbeatAck,
    Resumed,
    DavePrepareTransition {
        transition_id: String,
    },
    DaveExecuteTransition {
        transition_id: String,
    },
    DavePrepareEpoch {
        epoch: u64,
        transition_id: String,
    },
    DaveMlsExternalSender {
        external_sender_package: Vec<u8>,
    },
    DaveMlsProposals {
        operation_type: u8,
        proposals: Vec<u8>,
    },
    DaveMlsAnnounceCommit {
        transition_id: u64,
        commit: Vec<u8>,
    },
    DaveMlsWelcome {
        transition_id: u64,
        welcome: Vec<u8>,
    },
    AudioFrame {
        data: Vec<u8>,
        opcode: u8,
    },
    SpeakingUpdate {
        ssrc: u32,
        user_id: u64,
        speaking: bool,
    },
    ClientsConnect,
    ClientDisconnect,
    Closed,
    Ping,
    Pong,
    Unknown,
}
