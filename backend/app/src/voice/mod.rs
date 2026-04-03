pub mod gateway;
pub mod session;
pub mod udp;
pub mod rtp;

pub use gateway::VoiceGateway;
pub use gateway::VoiceEvent;
pub use session::DaveyVoiceSession;
pub use udp::VoiceUdpSocket;
pub use udp::IpDiscovery;
pub use rtp::RtpHeader;

use crate::state::{AppState, VoiceSession, VoiceUserMeta};
use anyhow::Result;
use twilight_model::{
    guild::Member,
    gateway::payload::incoming::VoiceStateUpdate,
    id::{marker::{ChannelMarker, GuildMarker, UserMarker}, Id},
};

pub type SpeakerId = Id<UserMarker>;

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

pub async fn join_voice(state: &AppState, guild_id: Id<GuildMarker>, channel_id: Id<ChannelMarker>) -> Result<VoiceSession> {
    state.voice_engine.join(state, guild_id, channel_id).await
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
