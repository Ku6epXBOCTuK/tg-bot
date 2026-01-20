use crate::config::Config;
use crate::state::manager::{StateManager, StreamEvent};
use crate::storage::sqlite::Storage;
use crate::twitch::api::TwitchApiClient;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

pub struct TwitchPoller {
    client: TwitchApiClient,
    state_manager: Arc<StateManager>,
    storage: Storage,
    interval_seconds: u64,
}

impl TwitchPoller {
    pub fn new(config: Config, state_manager: Arc<StateManager>, storage: Storage) -> Self {
        let client = TwitchApiClient::new(config.clone());
        let interval_seconds = config.polling_interval_seconds;

        Self {
            client,
            state_manager,
            storage,
            interval_seconds,
        }
    }

    pub async fn start_polling(&mut self) {
        info!(
            "Starting Twitch polling with {}s interval",
            self.interval_seconds
        );

        let mut interval = interval(Duration::from_secs(self.interval_seconds));

        loop {
            interval.tick().await;

            // Get all unique twitch_user_ids from subscriptions
            let user_ids = match self.storage.get_all_twitch_user_ids().await {
                Ok(ids) => ids,
                Err(e) => {
                    error!("Failed to get twitch user IDs: {}", e);
                    continue;
                }
            };

            if user_ids.is_empty() {
                continue;
            }

            // Query Twitch API for all streamers at once
            match self.client.get_streams(&user_ids).await {
                Ok(streams) => {
                    // Process each streamer
                    for user_id in &user_ids {
                        let is_online = streams.iter().any(|s| &s.user_id == user_id);

                        if is_online {
                            let stream = streams.iter().find(|s| &s.user_id == user_id).unwrap();

                            // Get target chat IDs for this streamer
                            let target_chat_ids = match self
                                .storage
                                .get_target_chat_ids_for_streamer(user_id)
                                .await
                            {
                                Ok(chat_ids) => chat_ids,
                                Err(e) => {
                                    error!(
                                        "Failed to get target chat IDs for streamer {}: {}",
                                        user_id, e
                                    );
                                    Vec::new()
                                }
                            };

                            let event = StreamEvent {
                                streamer_id: user_id.clone(),
                                streamer_login: stream.user_login.clone(),
                                streamer_name: stream.user_name.clone(),
                                event_id: format!("{}_{}", user_id, stream.started_at),
                                event_timestamp: stream.started_at.clone(),
                                started_at: stream.started_at.clone(),
                                target_chat_ids,
                            };

                            self.state_manager.handle_stream_online(event).await;
                        } else {
                            self.state_manager
                                .handle_stream_offline(
                                    user_id,
                                    "",
                                    "",
                                    &format!("poll_{}", user_id),
                                    &chrono::Utc::now().to_rfc3339(),
                                )
                                .await;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get streams: {}", e);
                }
            }
        }
    }
}
