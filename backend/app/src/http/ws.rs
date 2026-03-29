use crate::state::{AppEvent, AppState};
use axum::{extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State}, response::IntoResponse};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;

#[derive(Deserialize)]
struct ClientMsg {
    r#type: String,
    guild_id: Option<String>,
    session_id: Option<String>,
}

#[derive(serde::Serialize)]
struct EventEnvelope<'a> {
    r#type: &'a str,
    data: &'a AppEvent,
}

pub async fn ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: AppState, socket: WebSocket) {
    let mut rx = state.event_tx.subscribe();
    let mut ready_rx = state.ready_tx.subscribe();
    let (mut tx, mut rx_ws) = socket.split();
    let mut subscribed: Option<String> = None;
    let mut session_ok = false;
    let auth_required = matches!(state.config.activity_mode, crate::config::ActivityMode::Discord);
    let mut authorized_guilds: Option<Vec<String>> = None;

    if !auth_required {
        if let Some(ready) = state.ready_state.lock().await.clone() {
            if let Ok(text) = serde_json::to_string(&serde_json::json!({"type":"bot_ready","data":ready})) {
                let _ = tx.send(Message::text(text)).await;
            }
        }
    }

    loop {
        tokio::select! {
            msg = rx_ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMsg>(&text) {
                            match client_msg.r#type.as_str() {
                                "subscribe" => subscribed = client_msg.guild_id,
                                "session" => {
                                    if let Some(session_id) = client_msg.session_id {
                                        if let Some(session) = state.auth_session(&session_id).await {
                                            session_ok = true;
                                            authorized_guilds = Some(session.guild_ids.clone());
                                            let _ = tx.send(Message::text(serde_json::json!({"type":"session_ok"}).to_string())).await;
                                            if let Some(ready) = state.ready_state.lock().await.clone() {
                                                if !auth_required || session_ok {
                                                    if let Ok(text) = serde_json::to_string(&serde_json::json!({"type":"bot_ready","data":ready})) {
                                                        let _ = tx.send(Message::text(text)).await;
                                                    }
                                                }
                                            }
                                        } else {
                                            let _ = tx.send(Message::text(serde_json::json!({"type":"error","message":"invalid session"}).to_string())).await;
                                        }
                                    }
                                }
                                "ping" => {
                                    let _ = tx.send(Message::text(serde_json::json!({"type":"pong"}).to_string())).await;
                                }
                                _ => {
                                    let _ = tx.send(Message::text(serde_json::json!({"type":"error","message":"unknown message"}).to_string())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            ready = ready_rx.recv() => {
                if let Ok(status) = ready {
                    if !auth_required || session_ok {
                        if let Ok(text) = serde_json::to_string(&serde_json::json!({"type":"bot_ready","data":status})) {
                            let _ = tx.send(Message::text(text)).await;
                        }
                    }
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if should_forward(&subscribed, session_ok, authorized_guilds.as_deref(), auth_required, &event) {
                            let envelope = EventEnvelope { r#type: "event", data: &event };
                            if let Ok(text) = serde_json::to_string(&envelope) {
                                let _ = tx.send(Message::text(text)).await;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

fn should_forward(subscribed: &Option<String>, session_ok: bool, authorized_guilds: Option<&[String]>, auth_required: bool, event: &AppEvent) -> bool {
    if auth_required && !session_ok {
        return false;
    }
    match (subscribed, event) {
        (Some(guild), AppEvent::VoiceStateUpdate { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceSpeaking { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceAudioFrame { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceStream { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceReceiveTrace { trace }) => guild == &trace.guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == &trace.guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceSignalTrace { trace }) => guild == &trace.guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == &trace.guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceJoinState { state }) => match state {
            crate::state::VoiceJoinState::Idle { guild_id }
            | crate::state::VoiceJoinState::Joining { guild_id, .. }
            | crate::state::VoiceJoinState::Joined { guild_id, .. }
            | crate::state::VoiceJoinState::Unsupported { guild_id, .. }
            | crate::state::VoiceJoinState::Failed { guild_id, .. } => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        },
        (Some(guild), AppEvent::VoiceJoinRequested { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::VoiceJoinResult { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(guild), AppEvent::MessageCreate { guild_id, .. }) => guild == guild_id && authorized_guilds.map(|allowed| allowed.iter().any(|id| id == guild_id)).unwrap_or(true),
        (Some(_), AppEvent::Custom { .. }) => if auth_required { session_ok } else { true },
        (None, AppEvent::Custom { .. }) => if auth_required { session_ok } else { true },
        (None, _) => !auth_required,
    }
}
