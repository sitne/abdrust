use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Message(_) | Self::Anyhow(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status_code(),
            Json(json!({"error": self.to_string(), "status": self.status_code().as_u16()})),
        )
            .into_response()
    }
}

// ============================================================
// Voice-specific error types
// ============================================================

/// Voice gateway connection errors
#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("voice gateway connection failed: {0}")]
    ConnectionFailed(String),

    #[error("voice gateway handshake failed at stage '{stage}': {message}")]
    HandshakeFailed { stage: String, message: String },

    #[error("voice UDP socket error: {0}")]
    UdpError(String),

    #[error("voice RTP parse error: {0}")]
    RtpParseError(String),

    #[error("voice DAVE error: {0}")]
    DaveError(String),

    #[error("voice Opus error: {0}")]
    OpusError(String),

    #[error("voice IP discovery failed: {0}")]
    IpDiscoveryFailed(String),

    #[error("voice session expired: guild_id={guild_id}")]
    SessionExpired { guild_id: String },

    #[error("voice join timeout: guild_id={guild_id}, channel_id={channel_id}")]
    JoinTimeout {
        guild_id: String,
        channel_id: String,
    },

    #[error("voice reconnect failed after {attempts} attempts: {message}")]
    ReconnectFailed { attempts: u32, message: String },
}

impl VoiceError {
    pub fn stage(&self) -> &'static str {
        match self {
            Self::ConnectionFailed(_) => "connection",
            Self::HandshakeFailed { .. } => "handshake",
            Self::UdpError(_) => "udp",
            Self::RtpParseError(_) => "rtp_parse",
            Self::DaveError(_) => "dave",
            Self::OpusError(_) => "opus",
            Self::IpDiscoveryFailed(_) => "ip_discovery",
            Self::SessionExpired { .. } => "session_expired",
            Self::JoinTimeout { .. } => "join_timeout",
            Self::ReconnectFailed { .. } => "reconnect_failed",
        }
    }

    pub fn is_recoverable(&self) -> bool {
        match self {
            // These can potentially be recovered by reconnecting
            Self::ConnectionFailed(_)
            | Self::UdpError(_)
            | Self::IpDiscoveryFailed(_)
            | Self::SessionExpired { .. }
            | Self::ReconnectFailed { .. } => true,
            // These require a full re-join flow
            Self::HandshakeFailed { .. } | Self::JoinTimeout { .. } => true,
            // These are protocol-level errors, may not be recoverable
            Self::RtpParseError(_) | Self::DaveError(_) | Self::OpusError(_) => false,
        }
    }
}
