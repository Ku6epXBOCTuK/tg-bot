#![forbid(unsafe_code)]

mod config;
mod state;
mod storage;
mod telegram;
mod twitch;

use crate::config::Config;
use crate::state::manager::StateManager;
use crate::storage::sqlite::Storage;
use crate::telegram::bot::TelegramBot;
use crate::telegram::command_handler::CommandHandler;
use crate::telegram::commands::Command;
use crate::twitch::api::TwitchApiClient;
use crate::twitch::poller::TwitchPoller;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "twitch_telegram_bot=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Twitch Telegram Bot...");

    // Load configuration
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return;
        }
    };

    info!("Configuration loaded successfully");

    // Initialize storage
    let storage = match Storage::new(&config.database_url).await {
        Ok(storage) => storage,
        Err(e) => {
            error!("Failed to initialize storage: {}", e);
            return;
        }
    };

    info!("Storage initialized successfully");

    // Initialize Telegram bot
    let telegram_bot = TelegramBot::new(config.telegram_bot_token.clone(), &config);

    // Set bot commands for autocomplete
    match telegram_bot.set_my_commands().await {
        Ok(_) => info!("Bot commands set successfully"),
        Err(e) => error!("Failed to set bot commands: {}", e),
    }

    info!("Telegram bot initialized");

    // Initialize state manager
    let state_manager = Arc::new(StateManager::new(
        telegram_bot.clone(),
        storage.clone(),
        config.grace_period_online,
        config.grace_period_offline,
    ));

    info!("State manager initialized");

    // Load existing streamer configurations from database
    let streamer_configs = match storage.get_streamer_configs().await {
        Ok(configs) => configs,
        Err(e) => {
            error!("Failed to load streamer configurations: {}", e);
            Vec::new()
        }
    };

    if !streamer_configs.is_empty() {
        info!(
            "Loaded {} streamer configurations from database",
            streamer_configs.len()
        );
    }

    // Initialize Twitch poller
    let mut poller = TwitchPoller::new(config.clone(), state_manager.clone(), storage.clone());

    info!("Twitch poller initialized");

    // Start polling
    tokio::spawn(async move {
        poller.start_polling().await;
    });

    // Initialize Twitch API client for command handler
    let twitch_client = TwitchApiClient::new(config.clone());

    // Initialize command handler
    let command_handler = Arc::new(CommandHandler::new(
        storage.clone(),
        telegram_bot.clone(),
        twitch_client,
        config.clone(),
    ));

    info!("Command handler initialized");

    // Start Telegram bot with command handling
    let bot = Bot::new(config.telegram_bot_token.clone());
    let handler = command_handler.clone();

    // Create a shutdown signal channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn a task to handle Ctrl+C
    let ctrl_c_handle = tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C signal, shutting down...");
                let _ = shutdown_tx.send(true);
            }
            Err(err) => {
                error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    });

    // Start the bot with graceful shutdown
    let bot_handle = tokio::spawn(async move {
        teloxide::repl(bot, move |bot: Bot, msg: Message| {
            let handler = handler.clone();
            let shutdown_rx = shutdown_rx.clone();
            async move {
                // Check if shutdown signal was received
                if *shutdown_rx.borrow() {
                    return Ok(());
                }

                // Only handle private messages
                if msg.chat.is_private() {
                    if let Some(text) = msg.text() {
                        // Log essential info: user ID, username, and command
                        let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
                        let username = msg
                            .from
                            .as_ref()
                            .and_then(|u| u.username.clone())
                            .unwrap_or_else(|| user_id.to_string());

                        info!("User {} (ID: {}) sent: {}", username, user_id, text);

                        // Try to parse command using teloxide's built-in parser
                        info!("Attempting to parse command: {}", text);
                        match Command::parse(text, "") {
                            Ok(command) => {
                                info!("Parsed command: {:?}", command);
                                match handler.handle_command(command, user_id as i64).await {
                                    Ok(response) => {
                                        let result = bot
                                            .send_message(msg.chat.id, response)
                                            .parse_mode(teloxide::types::ParseMode::Html)
                                            .await;

                                        if let Err(e) = result {
                                            error!("Failed to send response: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Error handling command: {}", e);
                                        let error_msg = format!("❌ Ошибка: {}", e);
                                        if let Err(send_err) =
                                            bot.send_message(msg.chat.id, error_msg).await
                                        {
                                            error!("Failed to send error message: {}", send_err);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                info!("Failed to parse command: {}", e);
                                // Show help if command is not recognized
                                if text.starts_with('/') {
                                    let help_response = handler.handle_help();
                                    if let Err(send_err) = bot
                                        .send_message(msg.chat.id, help_response)
                                        .parse_mode(teloxide::types::ParseMode::Html)
                                        .await
                                    {
                                        error!("Failed to send help: {}", send_err);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
        })
        .await;
    });

    // Wait for either Ctrl+C or bot shutdown
    tokio::select! {
        _ = ctrl_c_handle => {
            info!("Ctrl+C received, shutting down...");
        }
        _ = bot_handle => {
            info!("Bot shutdown completed");
        }
    }

    info!("Bot stopped");
}
