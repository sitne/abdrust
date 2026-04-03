use crate::state::BotReady;
use twilight_gateway::MessageSender;
use twilight_model::gateway::{
    payload::outgoing::{update_presence::UpdatePresenceError, UpdatePresence},
    presence::{Activity, ActivityType, Status},
};

pub fn activity_from_ready(ready: &BotReady) -> Result<UpdatePresence, UpdatePresenceError> {
    let label = match ready.status.as_str() {
        "command_invoked" => "handling commands",
        "joining voice channel" => "joining voice channel",
        "voice connected" => "voice connected",
        "voice disconnected" => "voice disconnected",
        "ready" => "ready",
        other => other,
    };

    UpdatePresence::new(
        vec![Activity {
            application_id: None,
            assets: None,
            buttons: Vec::new(),
            created_at: None,
            details: Some("abdrust".to_string()),
            emoji: None,
            flags: None,
            id: None,
            instance: None,
            kind: ActivityType::Playing,
            name: label.to_string(),
            party: None,
            secrets: None,
            state: None,
            timestamps: None,
            url: None,
        }],
        false,
        None,
        Status::Online,
    )
}

pub fn send_status(sender: &MessageSender, label: &str) {
    let activity = BotReady {
        status: label.to_string(),
        application_id: String::new(),
        guild_id: None,
        commands: Vec::new(),
        voice_capabilities: Default::default(),
    };

    if let Ok(payload) = activity_from_ready(&activity) {
        let _ = sender.command(&payload);
    }
}

pub fn send_ready(sender: &MessageSender, ready: &BotReady) {
    if let Ok(payload) = activity_from_ready(ready) {
        let _ = sender.command(&payload);
    }
}
