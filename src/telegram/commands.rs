use teloxide::utils::command::BotCommands;
use thiserror::Error;

#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Start the bot and see welcome message")]
    Start,
    #[command(description = "Add a streamer to track")]
    Add(String),
    #[command(
        description = "Show your subscriptions and settings",
        rename = "my_settings"
    )]
    MySettings,
    #[command(
        description = "Set target chat for notifications",
        rename = "set_channel"
    )]
    SetChannel(String),
    #[command(description = "Set custom notification message", rename = "set_text")]
    SetText(String),
    #[command(
        description = "Add inline button (format: Text | URL)",
        rename = "add_button"
    )]
    AddButton(String),
    #[command(description = "Clear all inline buttons")]
    ClearButtons,
    #[command(description = "Send test notification to configured channel")]
    Test,
    #[command(description = "Preview notification in private chat")]
    Preview,
    #[command(description = "Remove streamer from your list")]
    Remove(String),
    #[command(description = "Show help")]
    Help,
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Invalid command format: {0}")]
    InvalidFormat(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Twitch API error: {0}")]
    TwitchApiError(String),
}
