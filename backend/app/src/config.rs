use anyhow::{Context, Result};
use http::HeaderValue;
use std::env;
use twilight_model::id::{
    marker::{ApplicationMarker, GuildMarker},
    Id,
};

#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    pub discord_client_id: String,
    pub discord_client_secret: String,
    pub discord_redirect_uri: String,
    pub discord_guild_id: Option<Id<GuildMarker>>,
    pub host: String,
    pub port: u16,
    pub frontend_dist: String,
    pub cors_allow_all: bool,
    pub cors_allowed_origins: Vec<HeaderValue>,
    pub activity_mode: ActivityMode,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActivityMode {
    Local,
    Discord,
}

impl Config {
    pub fn application_id(&self) -> Result<Id<ApplicationMarker>> {
        self.discord_client_id
            .parse::<u64>()
            .context("DISCORD_CLIENT_ID must be a number")
            .map(Id::new)
    }

    pub fn load_project_env() {
        let _ = dotenvy::from_path("../.env");
        let _ = dotenvy::from_path(".env");
    }

    pub fn from_env() -> Result<Self> {
        Self::load_project_env();
        let discord_token = env::var("DISCORD_TOKEN").context("DISCORD_TOKEN is required")?;
        let discord_client_id =
            env::var("DISCORD_CLIENT_ID").context("DISCORD_CLIENT_ID is required")?;
        let discord_client_secret =
            env::var("DISCORD_CLIENT_SECRET").context("DISCORD_CLIENT_SECRET is required")?;
        let discord_redirect_uri =
            env::var("DISCORD_REDIRECT_URI").unwrap_or_else(|_| "https://localhost".to_string());
        let discord_guild_id = env::var("DISCORD_GUILD_ID")
            .ok()
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("DISCORD_GUILD_ID must be a number")
            })
            .transpose()?
            .map(Id::new);
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .context("PORT must be a number")?;
        let frontend_dist =
            env::var("FRONTEND_DIST").unwrap_or_else(|_| "../frontend/dist".to_string());
        let activity_mode = match env::var("ACTIVITY_MODE")
            .unwrap_or_else(|_| "local".to_string())
            .to_lowercase()
            .as_str()
        {
            "discord" => ActivityMode::Discord,
            _ => ActivityMode::Local,
        };
        let cors_allow_all = env::var("CORS_ALLOW_ALL")
            .map(|v| v != "false")
            .unwrap_or(matches!(activity_mode, ActivityMode::Local));
        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|origin| !origin.is_empty())
                    .map(|origin| {
                        HeaderValue::from_str(origin).with_context(|| {
                            format!("CORS_ALLOWED_ORIGINS contains an invalid origin: {origin}")
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            discord_token,
            discord_client_id,
            discord_client_secret,
            discord_redirect_uri,
            discord_guild_id,
            host,
            port,
            frontend_dist,
            cors_allow_all,
            cors_allowed_origins,
            activity_mode,
        })
    }
}
