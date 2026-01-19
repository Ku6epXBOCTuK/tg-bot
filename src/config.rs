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
    pub telegram_channel_id: i64,
    #[allow(dead_code)]
    pub twitch_client_id: String,
    #[allow(dead_code)]
    pub twitch_client_secret: String,
    #[allow(dead_code)]
    pub webhook_base_url: String,
    pub webhook_secret: String,
    pub database_url: String,
    pub grace_period_online: u64,  // seconds
    pub grace_period_offline: u64, // seconds
    pub server_port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| ConfigError::MissingEnv("TELEGRAM_BOT_TOKEN".to_string()))?;

        let telegram_channel_id = env::var("TELEGRAM_CHANNEL_ID")
            .map_err(|_| ConfigError::MissingEnv("TELEGRAM_CHANNEL_ID".to_string()))?
            .parse::<i64>()
            .map_err(|e| {
                ConfigError::InvalidValue("TELEGRAM_CHANNEL_ID".to_string(), e.to_string())
            })?;

        let twitch_client_id = env::var("TWITCH_CLIENT_ID")
            .map_err(|_| ConfigError::MissingEnv("TWITCH_CLIENT_ID".to_string()))?;

        let twitch_client_secret = env::var("TWITCH_CLIENT_SECRET")
            .map_err(|_| ConfigError::MissingEnv("TWITCH_CLIENT_SECRET".to_string()))?;

        let webhook_base_url = env::var("WEBHOOK_BASE_URL")
            .map_err(|_| ConfigError::MissingEnv("WEBHOOK_BASE_URL".to_string()))?;

        let webhook_secret = env::var("WEBHOOK_SECRET")
            .map_err(|_| ConfigError::MissingEnv("WEBHOOK_SECRET".to_string()))?;

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

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|e| ConfigError::InvalidValue("SERVER_PORT".to_string(), e.to_string()))?;

        Ok(Config {
            telegram_bot_token,
            telegram_channel_id,
            twitch_client_id,
            twitch_client_secret,
            webhook_base_url,
            webhook_secret,
            database_url,
            grace_period_online,
            grace_period_offline,
            server_port,
        })
    }

    #[allow(dead_code)]
    pub fn twitch_webhook_url(&self) -> String {
        format!("{}/webhook/twitch", self.webhook_base_url)
    }
}
