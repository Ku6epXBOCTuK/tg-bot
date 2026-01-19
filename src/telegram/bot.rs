use chrono::{DateTime, Utc};
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId};
use thiserror::Error;
use tracing::{error, info};

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
    channel_id: ChatId,
    max_retries: u32,
}

impl TelegramBot {
    pub fn new(token: String, channel_id: i64) -> Self {
        let bot = Bot::new(token);
        Self {
            bot,
            channel_id: ChatId(channel_id),
            max_retries: 3,
        }
    }

    pub async fn send_stream_notification(
        &self,
        streamer_login: &str,
        streamer_name: &str,
        started_at: DateTime<Utc>,
    ) -> Option<i32> {
        let formatted_time = started_at.format("%H:%M %d.%m.%Y").to_string();

        let message = format!(
            "🔴 <b>Стрим начался!</b>\n\n\
            <b>Стример:</b> {} (@{})\n\
            <b>Начало:</b> {}\n\n\
            <a href=\"https://twitch.tv/{}\">Перейти на стрим</a>",
            streamer_name, streamer_login, formatted_time, streamer_login
        );

        for attempt in 1..=self.max_retries {
            match self
                .bot
                .send_message(self.channel_id, &message)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                Ok(sent_message) => {
                    let message_id = sent_message.id.0;
                    info!(
                        "Sent Telegram notification for {} (message_id: {})",
                        streamer_login, message_id
                    );
                    return Some(message_id);
                }
                Err(e) => {
                    error!(
                        "Attempt {} failed to send Telegram notification for {}: {}",
                        attempt, streamer_login, e
                    );
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!(
            "Failed to send Telegram notification after {} attempts",
            self.max_retries
        );
        None
    }

    #[allow(dead_code)]
    pub async fn delete_message(&self, message_id: i32) -> bool {
        for attempt in 1..=self.max_retries {
            match self
                .bot
                .delete_message(self.channel_id, MessageId(message_id))
                .await
            {
                Ok(_) => {
                    info!("Deleted Telegram message {}", message_id);
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
    pub async fn send_message(&self, text: &str) -> bool {
        for attempt in 1..=self.max_retries {
            match self
                .bot
                .send_message(self.channel_id, text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                Ok(_) => {
                    info!("Sent Telegram message: {}", text);
                    return true;
                }
                Err(e) => {
                    error!("Attempt {} failed to send Telegram message: {}", attempt, e);
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!(
            "Failed to send Telegram message after {} attempts",
            self.max_retries
        );
        false
    }
}
