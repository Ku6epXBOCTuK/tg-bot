mod config;
mod state;
mod storage;
mod telegram;
mod twitch;
mod web;

use crate::config::Config;
use crate::state::manager::StateManager;
use crate::storage::sqlite::Storage;
use crate::telegram::bot::TelegramBot;
use crate::twitch::api::TwitchApiClient;
use crate::web::routes::{create_router, AppState};
use axum::serve;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
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
    let telegram_bot = TelegramBot::new(
        config.telegram_bot_token.clone(),
        config.telegram_channel_id,
    );

    info!("Telegram bot initialized");

    // Initialize state manager
    let state_manager = Arc::new(StateManager::new(
        telegram_bot.clone(),
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

    // Initialize Twitch API client (not used in current version but available for future features)
    let _twitch_client = TwitchApiClient::new(config.clone());

    info!("Twitch API client initialized");

    // Create app state
    let app_state = AppState {
        config: config.clone(),
        state_manager: state_manager.clone(),
    };

    // Create and start web server
    let router = create_router(app_state);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));

    info!("Starting web server on http://{}", addr);

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to address: {}", e);
            return;
        }
    };

    let server =
        serve(listener, router.into_make_service()).with_graceful_shutdown(shutdown_signal());

    // Run the server
    if let Err(e) = server.await {
        error!("Server error: {}", e);
    }

    info!("Bot stopped");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Ctrl+C received, shutting down...");
        },
        _ = terminate => {
            info!("SIGTERM received, shutting down...");
        },
    }
}
