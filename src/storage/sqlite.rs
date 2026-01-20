use crate::state::models::StreamerConfig;
use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::path::Path;
use thiserror::Error;
use tracing::{error, info};

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum StorageError {
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Failed to parse date: {0}")]
    DateParse(String),
}

#[derive(Clone)]
pub struct Storage {
    pool: Pool<Sqlite>,
}

impl Storage {
    pub async fn new(database_url: &str) -> Result<Self, StorageError> {
        let (db_file, is_new) = Self::prepare_database_file(database_url)?;
        let pool = Self::create_pool(database_url).await?;
        Self::run_migrations(&pool, &db_file, is_new).await?;
        Self::log_initialization_status(is_new).await;

        Ok(Self { pool })
    }

    async fn log_initialization_status(is_new: bool) {
        if is_new {
            info!("Database created and migrations applied successfully");
        } else {
            info!("Database initialized and migrations verified");
        }
    }

    fn prepare_database_file(database_url: &str) -> Result<(String, bool), StorageError> {
        // Extract file path from URL (e.g., "sqlite:twitch_bot.db" -> "twitch_bot.db")
        let db_file = if let Some(stripped) = database_url.strip_prefix("sqlite:") {
            stripped
        } else {
            database_url
        };

        // Check if database file exists
        let db_path = Path::new(db_file);
        let is_new = !db_path.exists();

        // Create parent directories if needed
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    StorageError::Migration(format!("Failed to create parent directories: {}", e))
                })?;
            }
        }

        // Create the database file if it doesn't exist
        if is_new {
            info!("Creating new database file: {}", db_file);
            // Create an empty file first
            std::fs::File::create(db_file).map_err(|e| {
                StorageError::Migration(format!("Failed to create database file: {}", e))
            })?;
        } else {
            info!("Using existing database file: {}", db_file);
        }

        Ok((db_file.to_string(), is_new))
    }

    async fn create_pool(database_url: &str) -> Result<Pool<Sqlite>, StorageError> {
        match SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
        {
            Ok(pool) => Ok(pool),
            Err(e) => {
                // Try to provide more helpful error information
                let db_file = if let Some(stripped) = database_url.strip_prefix("sqlite:") {
                    stripped
                } else {
                    database_url
                };
                let db_path = Path::new(db_file);
                let parent = db_path.parent().unwrap_or_else(|| Path::new("."));

                error!("Failed to connect to database: {}", e);
                error!("Database file: {}", db_file);
                error!("Parent directory: {:?}", parent);
                error!("Parent exists: {}", parent.exists());
                error!(
                    "Parent is writable: {}",
                    std::fs::metadata(parent)
                        .map(|m| m.permissions().readonly())
                        .unwrap_or(true)
                );

                Err(StorageError::Sqlx(e))
            }
        }
    }

    async fn run_migrations(
        pool: &Pool<Sqlite>,
        db_file: &str,
        is_new: bool,
    ) -> Result<(), StorageError> {
        match sqlx::migrate!("./migrations").run(pool).await {
            Ok(_) => {
                if is_new {
                    info!("Database created and migrations applied successfully");
                } else {
                    info!("Database initialized and migrations verified");
                }
                Ok(())
            }
            Err(e) => {
                // Check if it's a migration conflict
                let error_str = e.to_string();
                if error_str.contains("migration") && error_str.contains("previously applied") {
                    Self::handle_migration_conflict(db_file, pool).await
                } else {
                    Err(StorageError::Migration(e.to_string()))
                }
            }
        }
    }

    #[allow(dead_code)]
    async fn handle_migration_conflict(
        db_file: &str,
        _pool: &Pool<Sqlite>,
    ) -> Result<(), StorageError> {
        // Migration conflict detected - backup and recreate
        info!("Migration conflict detected, backing up existing database...");

        // Backup the existing database file
        let backup_path = format!(
            "{}.backup.{}",
            db_file,
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        std::fs::copy(db_file, &backup_path)
            .map_err(|e| StorageError::Migration(format!("Failed to backup database: {}", e)))?;

        info!("Database backed up to: {}", backup_path);

        // Remove the old database file
        std::fs::remove_file(db_file).map_err(|e| {
            StorageError::Migration(format!("Failed to remove old database: {}", e))
        })?;

        // Create new database file
        std::fs::File::create(db_file).map_err(|e| {
            StorageError::Migration(format!("Failed to create new database file: {}", e))
        })?;

        // Reconnect to the new database
        let new_pool = Self::create_pool(&format!("sqlite:{}", db_file)).await?;

        // Run migrations again on the new database
        sqlx::migrate!("./migrations")
            .run(&new_pool)
            .await
            .map_err(|e| StorageError::Migration(e.to_string()))?;

        info!("Database recreated and migrations applied successfully");
        Ok(())
    }
    pub async fn get_streamer_configs(&self) -> Result<Vec<StreamerConfig>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                streamer_id,
                streamer_login,
                streamer_name,
                created_at,
                eventsub_subscription_id,
                online_subscription_id,
                offline_subscription_id
            FROM streamers
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut configs = Vec::new();
        for row in rows {
            let created_at_str: String = row.get("created_at");
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            configs.push(StreamerConfig {
                streamer_id: row.get("streamer_id"),
                streamer_login: row.get("streamer_login"),
                streamer_name: row.get("streamer_name"),
                created_at,
                eventsub_subscription_id: row.get("eventsub_subscription_id"),
                online_subscription_id: row.get("online_subscription_id"),
                offline_subscription_id: row.get("offline_subscription_id"),
            });
        }

        Ok(configs)
    }
}

// User subscription model
#[derive(Clone, Debug)]
pub struct UserSubscription {
    pub id: i64,
    pub twitch_user_id: String,
}

// Notification settings model
#[derive(Clone, Debug)]
pub struct NotificationSettings {
    pub target_chat_id: String,
    pub custom_message: String,
    pub inline_buttons_json: Option<String>,
}

impl Storage {
    // User subscription methods
    pub async fn add_user_subscription(
        &self,
        user_telegram_id: i64,
        twitch_user_id: &str,
    ) -> Result<i64, StorageError> {
        let result = sqlx::query(
            r#"
            INSERT INTO user_subscriptions (user_telegram_id, twitch_user_id)
            VALUES (?, ?)
            ON CONFLICT(user_telegram_id, twitch_user_id) DO UPDATE SET
                twitch_user_id = excluded.twitch_user_id
            RETURNING id
            "#,
        )
        .bind(user_telegram_id)
        .bind(twitch_user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.get("id"))
    }

    pub async fn get_user_subscriptions(
        &self,
        user_telegram_id: i64,
    ) -> Result<Vec<UserSubscription>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT id, twitch_user_id
            FROM user_subscriptions
            WHERE user_telegram_id = ?
            "#,
        )
        .bind(user_telegram_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| UserSubscription {
                id: row.get("id"),
                twitch_user_id: row.get("twitch_user_id"),
            })
            .collect())
    }

    pub async fn get_all_twitch_user_ids(&self) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT twitch_user_id
            FROM user_subscriptions
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get("twitch_user_id"))
            .collect())
    }

    pub async fn delete_user_subscription(
        &self,
        user_telegram_id: i64,
        twitch_user_id: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            DELETE FROM user_subscriptions
            WHERE user_telegram_id = ? AND twitch_user_id = ?
            "#,
        )
        .bind(user_telegram_id)
        .bind(twitch_user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Notification settings methods
    pub async fn save_notification_settings(
        &self,
        subscription_id: i64,
        target_chat_id: &str,
        custom_message: &str,
        inline_buttons_json: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO notification_settings (subscription_id, target_chat_id, custom_message, inline_buttons_json)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(subscription_id) DO UPDATE SET
                target_chat_id = excluded.target_chat_id,
                custom_message = excluded.custom_message,
                inline_buttons_json = excluded.inline_buttons_json
            "#,
        )
        .bind(subscription_id)
        .bind(target_chat_id)
        .bind(custom_message)
        .bind(inline_buttons_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_notification_settings(
        &self,
        subscription_id: i64,
    ) -> Result<Option<NotificationSettings>, StorageError> {
        let row = sqlx::query(
            r#"
            SELECT target_chat_id, custom_message, inline_buttons_json
            FROM notification_settings
            WHERE subscription_id = ?
            "#,
        )
        .bind(subscription_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| NotificationSettings {
            target_chat_id: row.get("target_chat_id"),
            custom_message: row.get("custom_message"),
            inline_buttons_json: row.get("inline_buttons_json"),
        }))
    }

    pub async fn get_all_notification_settings_for_user(
        &self,
        user_telegram_id: i64,
    ) -> Result<Vec<(i64, String, NotificationSettings)>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                us.id as subscription_id,
                us.twitch_user_id,
                ns.target_chat_id,
                ns.custom_message,
                ns.inline_buttons_json
            FROM user_subscriptions us
            LEFT JOIN notification_settings ns ON us.id = ns.subscription_id
            WHERE us.user_telegram_id = ?
            "#,
        )
        .bind(user_telegram_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let subscription_id: i64 = row.get("subscription_id");
                let twitch_user_id: String = row.get("twitch_user_id");
                let target_chat_id: String = row.get("target_chat_id");
                let custom_message: String = row.get("custom_message");
                let inline_buttons_json: Option<String> = row.get("inline_buttons_json");

                (
                    subscription_id,
                    twitch_user_id,
                    NotificationSettings {
                        target_chat_id,
                        custom_message,
                        inline_buttons_json,
                    },
                )
            })
            .collect())
    }

    pub async fn get_target_chat_ids_for_streamer(
        &self,
        twitch_user_id: &str,
    ) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT ns.target_chat_id
            FROM user_subscriptions us
            INNER JOIN notification_settings ns ON us.id = ns.subscription_id
            WHERE us.twitch_user_id = ?
            "#,
        )
        .bind(twitch_user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get("target_chat_id"))
            .collect())
    }
}
