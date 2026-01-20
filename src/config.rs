use std::env;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing environment variable: {0}")]
    MissingEnv(String),
    #[error("Invalid value for environment variable {0}: {1}")]
    InvalidValue(String, String),
}

#[derive(Clone, Debug)]
pub struct Config {
    pub telegram_bot_token: String,
    pub twitch_client_id: String,
    pub twitch_client_secret: String,
    pub database_url: String,
    pub grace_period_online: u64,  // seconds
    pub grace_period_offline: u64, // seconds
    pub polling_interval_seconds: u64,
    // Validation and limits
    pub max_retries: u32,
    pub retry_delay_seconds: u64,
    pub max_buttons_per_subscription: usize,
    pub max_message_length: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| ConfigError::MissingEnv("TELEGRAM_BOT_TOKEN".to_string()))?;

        let twitch_client_id = env::var("TWITCH_CLIENT_ID")
            .map_err(|_| ConfigError::MissingEnv("TWITCH_CLIENT_ID".to_string()))?;

        let twitch_client_secret = env::var("TWITCH_CLIENT_SECRET")
            .map_err(|_| ConfigError::MissingEnv("TWITCH_CLIENT_SECRET".to_string()))?;

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:twitch_bot.db".to_string());

        let grace_period_online = env::var("GRACE_PERIOD_ONLINE")
            .unwrap_or_else(|_| "180".to_string()) // 3 minutes
            .parse::<u64>()
            .map_err(|e| {
                ConfigError::InvalidValue("GRACE_PERIOD_ONLINE".to_string(), e.to_string())
            })?;

        let grace_period_offline = env::var("GRACE_PERIOD_OFFLINE")
            .unwrap_or_else(|_| "600".to_string()) // 10 minutes
            .parse::<u64>()
            .map_err(|e| {
                ConfigError::InvalidValue("GRACE_PERIOD_OFFLINE".to_string(), e.to_string())
            })?;

        let polling_interval_seconds = env::var("POLLING_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse::<u64>()
            .map_err(|e| {
                ConfigError::InvalidValue("POLLING_INTERVAL_SECONDS".to_string(), e.to_string())
            })?;

        let max_retries = env::var("MAX_RETRIES")
            .unwrap_or_else(|_| "3".to_string())
            .parse::<u32>()
            .map_err(|e| ConfigError::InvalidValue("MAX_RETRIES".to_string(), e.to_string()))?;

        let retry_delay_seconds = env::var("RETRY_DELAY_SECONDS")
            .unwrap_or_else(|_| "2".to_string())
            .parse::<u64>()
            .map_err(|e| {
                ConfigError::InvalidValue("RETRY_DELAY_SECONDS".to_string(), e.to_string())
            })?;

        let max_buttons_per_subscription = env::var("MAX_BUTTONS_PER_SUBSCRIPTION")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<usize>()
            .map_err(|e| {
                ConfigError::InvalidValue("MAX_BUTTONS_PER_SUBSCRIPTION".to_string(), e.to_string())
            })?;

        let max_message_length = env::var("MAX_MESSAGE_LENGTH")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<usize>()
            .map_err(|e| {
                ConfigError::InvalidValue("MAX_MESSAGE_LENGTH".to_string(), e.to_string())
            })?;

        Ok(Config {
            telegram_bot_token,
            twitch_client_id,
            twitch_client_secret,
            database_url,
            grace_period_online,
            grace_period_offline,
            polling_interval_seconds,
            max_retries,
            retry_delay_seconds,
            max_buttons_per_subscription,
            max_message_length,
        })
    }
}
