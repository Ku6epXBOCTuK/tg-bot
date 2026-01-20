use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId};
use teloxide::utils::command::BotCommands;
use thiserror::Error;
use tracing::{error, info};

use crate::telegram::commands::Command;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum TelegramError {
    #[error("Teloxide error: {0}")]
    Teloxide(#[from] teloxide::RequestError),
    #[error("Failed to send message")]
    SendFailed,
    #[error("Failed to delete message")]
    DeleteFailed,
}

#[derive(Clone)]
pub struct TelegramBot {
    bot: Bot,
    max_retries: u32,
}

impl TelegramBot {
    pub fn new(token: String) -> Self {
        let bot = Bot::new(token);
        Self {
            bot,
            max_retries: 3,
        }
    }

    pub async fn set_my_commands(&self) -> Result<(), TelegramError> {
        // Get commands and descriptions from the Command enum
        let commands = Command::bot_commands();

        match self.bot.set_my_commands(commands).await {
            Ok(_) => {
                info!("Telegram bot commands set successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to set bot commands: {}", e);
                Err(TelegramError::Teloxide(e))
            }
        }
    }

    pub async fn send_stream_notification(
        &self,
        chat_id: i64,
        streamer_login: &str,
        custom_message: &str,
        inline_buttons: Option<Vec<(String, String)>>,
    ) -> Option<i32> {
        let message = custom_message.to_string();

        for attempt in 1..=self.max_retries {
            let mut request = self
                .bot
                .send_message(ChatId(chat_id), &message)
                .parse_mode(teloxide::types::ParseMode::Html);

            if let Some(buttons) = &inline_buttons {
                let keyboard_results: Result<Vec<_>, _> = buttons
                    .clone()
                    .into_iter()
                    .map(|(text, url)| {
                        Url::parse(&url)
                            .map(|parsed_url| vec![InlineKeyboardButton::url(text, parsed_url)])
                            .inspect_err(|&e| {
                                error!("Failed to parse URL '{}': {}", url, e);
                            })
                    })
                    .collect();

                match keyboard_results {
                    Ok(keyboard) => {
                        let keyboard = InlineKeyboardMarkup::new(keyboard);
                        request = request.reply_markup(keyboard);
                    }
                    Err(_) => {
                        error!(
                            "Failed to create inline keyboard for streamer {}: invalid URL(s)",
                            streamer_login
                        );
                        // Continue without buttons
                    }
                }
            }

            match request.await {
                Ok(sent_message) => {
                    let message_id = sent_message.id.0;
                    info!(
                        "Sent Telegram notification to {} for {} (message_id: {})",
                        chat_id, streamer_login, message_id
                    );
                    return Some(message_id);
                }
                Err(e) => {
                    error!(
                        "Attempt {} failed to send Telegram notification to {}: {}",
                        attempt, chat_id, e
                    );
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!(
            "Failed to send Telegram notification to {} after {} attempts",
            chat_id, self.max_retries
        );
        None
    }

    #[allow(dead_code)]
    pub async fn delete_message(&self, chat_id: i64, message_id: i32) -> bool {
        for attempt in 1..=self.max_retries {
            match self
                .bot
                .delete_message(ChatId(chat_id), MessageId(message_id))
                .await
            {
                Ok(_) => {
                    info!(
                        "Deleted Telegram message {} in chat {}",
                        message_id, chat_id
                    );
                    return true;
                }
                Err(e) => {
                    info!(
                        "Attempt {} failed to delete Telegram message {}: {}",
                        attempt, message_id, e
                    );
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!(
            "Failed to delete Telegram message {} after {} attempts",
            message_id, self.max_retries
        );
        false
    }

    #[allow(dead_code)]
    pub async fn send_message(&self, chat_id: i64, text: &str) -> bool {
        for attempt in 1..=self.max_retries {
            match self
                .bot
                .send_message(ChatId(chat_id), text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                Ok(_) => {
                    info!("Sent Telegram message to {}: {}", chat_id, text);
                    return true;
                }
                Err(e) => {
                    error!(
                        "Attempt {} failed to send Telegram message to {}: {}",
                        attempt, chat_id, e
                    );
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!(
            "Failed to send Telegram message to {} after {} attempts",
            chat_id, self.max_retries
        );
        false
    }
}
