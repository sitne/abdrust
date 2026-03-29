use anyhow::{Context, Result};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = abdrust::config::Config::from_env()?;
    let client = twilight_http::Client::new(config.discord_token.clone());
    let app_id = config.application_id()?;
    let keep: HashSet<String> = [
        abdrust::bot::commands::DEBUG_COMMAND_NAME.to_string(),
        abdrust::bot::commands::ENTRY_POINT_COMMAND_NAME.to_string(),
    ]
        .into_iter()
        .collect();
    let keep_only_debug = std::env::var("KEEP_DEBUG_COMMAND_ONLY")
        .map(|v| v != "false")
        .unwrap_or(true);

    let global_commands = client
        .interaction(app_id)
        .global_commands()
        .await
        .context("failed to fetch global commands")?
        .models()
        .await
        .context("failed to read global commands")?;

    for command in global_commands {
        if keep_only_debug && !keep.contains(&command.name) {
            let Some(command_id) = command.id else {
                tracing::warn!(name = %command.name, "skipping global command without id");
                continue;
            };
            client
                .interaction(app_id)
                .delete_global_command(command_id)
                .await
                .context("failed to delete global command")?;
            println!("deleted global command: {}", command.name);
        }
    }

    if let Some(guild_id) = config.discord_guild_id {
        let guild_commands = client
            .interaction(app_id)
            .guild_commands(guild_id)
            .await
            .context("failed to fetch guild commands")?
            .models()
            .await
            .context("failed to read guild commands")?;

        for command in guild_commands {
            if keep_only_debug && !keep.contains(&command.name) {
                let Some(command_id) = command.id else {
                    tracing::warn!(name = %command.name, "skipping guild command without id");
                    continue;
                };
                client
                    .interaction(app_id)
                    .delete_guild_command(guild_id, command_id)
                    .await
                    .context("failed to delete guild command")?;
                println!("deleted guild command: {}", command.name);
            }
        }
    }

    Ok(())
}
