use crate::config::Config;
use anyhow::Result;
use twilight_model::application::{
    command::{Command, CommandType},
    command::{CommandOption, CommandOptionType},
    interaction::InteractionContextType,
};

pub const DEBUG_COMMAND_NAME: &str = "abdrust-debug";
pub const ENTRY_POINT_COMMAND_NAME: &str = "abdrust-launch";

#[allow(deprecated)]
pub fn build_commands(config: &Config) -> Result<Vec<Command>> {
    Ok(vec![
        Command {
            application_id: Some(config.application_id()?),
            contexts: Some(vec![InteractionContextType::Guild]),
            default_member_permissions: Some(twilight_model::guild::Permissions::empty()),
            dm_permission: Some(false),
            description: "Verify the bot and activity pipeline".to_string(),
            description_localizations: None,
            guild_id: config.discord_guild_id,
            id: None,
            integration_types: None,
            kind: CommandType::ChatInput,
            name: DEBUG_COMMAND_NAME.to_string(),
            name_localizations: None,
            nsfw: Some(false),
            options: Vec::new(),
            version: twilight_model::id::Id::new(1),
        },
        Command {
            application_id: Some(config.application_id()?),
            contexts: Some(vec![InteractionContextType::Guild]),
            default_member_permissions: Some(twilight_model::guild::Permissions::empty()),
            dm_permission: Some(false),
            description: "Show voice and DAVE diagnostics".to_string(),
            description_localizations: None,
            guild_id: config.discord_guild_id,
            id: None,
            integration_types: None,
            kind: CommandType::ChatInput,
            name: "voice-diag".to_string(),
            name_localizations: None,
            nsfw: Some(false),
            options: Vec::new(),
            version: twilight_model::id::Id::new(1),
        },
        Command {
            application_id: Some(config.application_id()?),
            contexts: Some(vec![InteractionContextType::Guild]),
            default_member_permissions: Some(twilight_model::guild::Permissions::empty()),
            dm_permission: Some(false),
            description: "Join or leave the current voice channel".to_string(),
            description_localizations: None,
            guild_id: config.discord_guild_id,
            id: None,
            integration_types: None,
            kind: CommandType::ChatInput,
            name: "voice".to_string(),
            name_localizations: None,
            nsfw: Some(false),
            options: vec![
                CommandOption {
                    autocomplete: None,
                    channel_types: None,
                    choices: None,
                    description: "Join the current voice channel".to_string(),
                    description_localizations: None,
                    kind: CommandOptionType::SubCommand,
                    max_length: None,
                    max_value: None,
                    min_length: None,
                    min_value: None,
                    name: "join".to_string(),
                    name_localizations: None,
                    options: Some(vec![]),
                    required: None,
                },
                CommandOption {
                    autocomplete: None,
                    channel_types: None,
                    choices: None,
                    description: "Leave the voice channel".to_string(),
                    description_localizations: None,
                    kind: CommandOptionType::SubCommand,
                    max_length: None,
                    max_value: None,
                    min_length: None,
                    min_value: None,
                    name: "leave".to_string(),
                    name_localizations: None,
                    options: Some(vec![]),
                    required: None,
                },
                CommandOption {
                    autocomplete: None,
                    channel_types: None,
                    choices: None,
                    description: "Show current voice session".to_string(),
                    description_localizations: None,
                    kind: CommandOptionType::SubCommand,
                    max_length: None,
                    max_value: None,
                    min_length: None,
                    min_value: None,
                    name: "status".to_string(),
                    name_localizations: None,
                    options: Some(vec![]),
                    required: None,
                },
            ],
            version: twilight_model::id::Id::new(1),
        }
    ])
}

pub async fn ensure_entry_point_command(config: &Config) -> Result<()> {
    let client = reqwest::Client::new();
    let app_id = config.application_id()?;
    let payload = serde_json::json!({
        "name": ENTRY_POINT_COMMAND_NAME,
        "description": "Launch abdrust in Discord",
        "type": 4,
        "handler": 2,
        "integration_types": [0, 1],
        "contexts": [0, 1, 2],
    });

    let existing = client
        .get(format!(
            "https://discord.com/api/v10/applications/{}/commands",
            app_id.get()
        ))
        .header(reqwest::header::AUTHORIZATION, format!("Bot {}", config.discord_token))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<serde_json::Value>>()
        .await?;

    let existing_entry_point = existing.iter().find(|command| {
        command.get("type").and_then(|value| value.as_u64()) == Some(4)
    });

    match existing_entry_point.and_then(|command| command.get("id")).and_then(|id| id.as_str()) {
        Some(command_id) => {
            client
                .patch(format!(
                    "https://discord.com/api/v10/applications/{}/commands/{}",
                    app_id.get(),
                    command_id
                ))
                .header(reqwest::header::AUTHORIZATION, format!("Bot {}", config.discord_token))
                .json(&payload)
                .send()
                .await?
                .error_for_status()?;
        }
        None => {
            client
                .post(format!(
                    "https://discord.com/api/v10/applications/{}/commands",
                    app_id.get()
                ))
                .header(reqwest::header::AUTHORIZATION, format!("Bot {}", config.discord_token))
                .json(&payload)
                .send()
                .await?
                .error_for_status()?;
        }
    }

    Ok(())
}
