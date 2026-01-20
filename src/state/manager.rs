use crate::state::models::{StreamState, StreamStatus};
use crate::telegram::bot::TelegramBot;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

#[derive(Clone)]
pub struct StateManager {
    states: Arc<RwLock<HashMap<String, StreamState>>>,
    telegram_bot: Arc<Mutex<TelegramBot>>,
    grace_period_online: u64,
    grace_period_offline: u64,
}

impl StateManager {
    pub fn new(
        telegram_bot: TelegramBot,
        grace_period_online: u64,
        grace_period_offline: u64,
    ) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            telegram_bot: Arc::new(Mutex::new(telegram_bot)),
            grace_period_online,
            grace_period_offline,
        }
    }

    #[allow(dead_code)]
    pub async fn get_state(&self, streamer_id: &str) -> Option<StreamState> {
        let states = self.states.read().await;
        states.get(streamer_id).cloned()
    }

    pub async fn ensure_state(&self, streamer_id: &str, streamer_login: &str, streamer_name: &str) {
        let mut states = self.states.write().await;
        if !states.contains_key(streamer_id) {
            states.insert(
                streamer_id.to_string(),
                StreamState::new(
                    streamer_id.to_string(),
                    streamer_login.to_string(),
                    streamer_name.to_string(),
                ),
            );
        }
    }

    pub async fn handle_stream_online(
        &self,
        streamer_id: &str,
        streamer_login: &str,
        streamer_name: &str,
        event_id: &str,
        event_timestamp: &str,
        started_at: &str,
    ) {
        info!(
            "Handling stream.online for {} (ID: {})",
            streamer_login, streamer_id
        );

        self.ensure_state(streamer_id, streamer_login, streamer_name)
            .await;

        let mut states = self.states.write().await;
        let state = states.get_mut(streamer_id).unwrap();

        // Check for duplicate events
        if state.is_duplicate_event(event_id, event_timestamp) {
            info!(
                "Duplicate stream.online event detected for {}",
                streamer_login
            );
            return;
        }

        state.update_event_info(event_id.to_string(), event_timestamp);

        match state.status {
            StreamStatus::Offline => {
                // Start grace period
                state.status = StreamStatus::OnlinePending;
                state.pending_started_at = Some(
                    DateTime::parse_from_rfc3339(started_at)
                        .unwrap()
                        .with_timezone(&Utc),
                );
                state.grace_period_start = Some(Utc::now());

                info!(
                    "Stream {} entered OnlinePending state, grace period: {}s",
                    streamer_login, self.grace_period_online
                );

                let state_manager = self.clone();
                let streamer_id_clone = streamer_id.to_string();
                let streamer_login_clone = streamer_login.to_string();
                let grace_period_start = state.grace_period_start;

                tokio::spawn(async move {
                    sleep(Duration::from_secs(state_manager.grace_period_online)).await;

                    let mut states = state_manager.states.write().await;
                    if let Some(state) = states.get_mut(&streamer_id_clone) {
                        // Check if grace period is still valid (race condition protection)
                        if state.grace_period_start != grace_period_start {
                            info!(
                                "Grace period for {} is no longer valid, ignoring timer",
                                streamer_login_clone
                            );
                            return;
                        }

                        if state.status == StreamStatus::OnlinePending {
                            info!(
                                "Grace period ended for {}, confirming online",
                                streamer_login_clone
                            );

                            // Send notification
                            let telegram_bot = state_manager.telegram_bot.lock().await;
                            if let Some(_started_at) = state.pending_started_at {
                                if let Some(message_id) = telegram_bot
                                    .send_stream_notification(
                                        0, // chat_id will be determined by user settings
                                        &state.streamer_login,
                                        "🔴 <b>Стрим начался!</b>",
                                        None,
                                    )
                                    .await
                                {
                                    state.telegram_message_id = Some(message_id);
                                    state.started_at = state.pending_started_at;
                                    state.status = StreamStatus::Online;
                                    state.pending_started_at = None;
                                    state.grace_period_start = None;

                                    info!(
                                        "Stream {} confirmed online, message ID: {}",
                                        streamer_login_clone, message_id
                                    );
                                } else {
                                    error!(
                                        "Failed to send Telegram notification for {}",
                                        streamer_login_clone
                                    );
                                    state.status = StreamStatus::Offline;
                                    state.pending_started_at = None;
                                    state.grace_period_start = None;
                                }
                            }
                        } else if state.status == StreamStatus::Online {
                            // Stream already confirmed online (received another online event)
                            info!(
                                "Stream {} already online, ignoring grace period completion",
                                streamer_login_clone
                            );
                        } else {
                            // Stream went offline during grace period
                            info!(
                                "Stream {} went offline during grace period, cancelling notification",
                                streamer_login_clone
                            );
                        }
                    }
                });
            }
            StreamStatus::OnlinePending => {
                // Already pending, ignore (duplicate event)
                info!(
                    "Stream {} already in OnlinePending state, ignoring",
                    streamer_login
                );
            }
            StreamStatus::Online => {
                // Already online, ignore (duplicate event)
                info!("Stream {} already online, ignoring", streamer_login);
            }
            StreamStatus::OfflinePending => {
                // Stream was offline pending, cancel the offline timer and stay online
                info!(
                    "Stream {} was OfflinePending, cancelling offline timer",
                    streamer_login
                );
                state.status = StreamStatus::Online;
                state.grace_period_start = None;
            }
        }
    }

    pub async fn handle_stream_offline(
        &self,
        streamer_id: &str,
        streamer_login: &str,
        streamer_name: &str,
        event_id: &str,
        event_timestamp: &str,
    ) {
        info!(
            "Handling stream.offline for {} (ID: {})",
            streamer_login, streamer_id
        );

        self.ensure_state(streamer_id, streamer_login, streamer_name)
            .await;

        let mut states = self.states.write().await;
        let state = states.get_mut(streamer_id).unwrap();

        // Check for duplicate events
        if state.is_duplicate_event(event_id, event_timestamp) {
            info!(
                "Duplicate stream.offline event detected for {}",
                streamer_login
            );
            return;
        }

        state.update_event_info(event_id.to_string(), event_timestamp);

        match state.status {
            StreamStatus::Online => {
                // Start grace period for offline
                state.status = StreamStatus::OfflinePending;
                state.grace_period_start = Some(Utc::now());

                info!(
                    "Stream {} entered OfflinePending state, grace period: {}s",
                    streamer_login, self.grace_period_offline
                );

                let state_manager = self.clone();
                let streamer_id_clone = streamer_id.to_string();
                let streamer_login_clone = streamer_login.to_string();
                let grace_period_start = state.grace_period_start;

                tokio::spawn(async move {
                    sleep(Duration::from_secs(state_manager.grace_period_offline)).await;

                    let mut states = state_manager.states.write().await;
                    if let Some(state) = states.get_mut(&streamer_id_clone) {
                        // Check if grace period is still valid (race condition protection)
                        if state.grace_period_start != grace_period_start {
                            info!(
                                "Grace period for {} is no longer valid, ignoring timer",
                                streamer_login_clone
                            );
                            return;
                        }

                        if state.status == StreamStatus::OfflinePending {
                            info!(
                                "Grace period ended for {}, confirming offline",
                                streamer_login_clone
                            );

                            // Delete notification
                            // Note: Message deletion is not currently supported in the per-user/channel architecture
                            // Each user has their own notification settings, so we can't determine which chat to delete from
                            // This is a known limitation of the current design
                            if let Some(_message_id) = state.telegram_message_id {
                                info!(
                                    "Stream {} offline, but message deletion is not supported in per-user/channel mode",
                                    streamer_login_clone
                                );
                            }

                            // Reset state
                            state.status = StreamStatus::Offline;
                            state.started_at = None;
                            state.telegram_message_id = None;
                            state.grace_period_start = None;

                            info!("Stream {} confirmed offline", streamer_login_clone);
                        } else if state.status == StreamStatus::Offline {
                            // Already offline
                            info!(
                                "Stream {} already offline, ignoring grace period completion",
                                streamer_login_clone
                            );
                        } else {
                            // Stream went back online during grace period
                            info!(
                                "Stream {} went back online during grace period, cancelling deletion",
                                streamer_login_clone
                            );
                        }
                    }
                });
            }
            StreamStatus::OfflinePending => {
                // Already pending offline, ignore
                info!(
                    "Stream {} already in OfflinePending state, ignoring",
                    streamer_login
                );
            }
            StreamStatus::Offline => {
                // Already offline, ignore
                info!("Stream {} already offline, ignoring", streamer_login);
            }
            StreamStatus::OnlinePending => {
                // Stream was online pending, cancel the online timer and go offline
                info!(
                    "Stream {} was OnlinePending, cancelling online timer",
                    streamer_login
                );
                state.status = StreamStatus::Offline;
                state.pending_started_at = None;
                state.grace_period_start = None;
            }
        }
    }

    #[allow(dead_code)]
    pub async fn get_all_states(&self) -> HashMap<String, StreamState> {
        let states = self.states.read().await;
        states.clone()
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) {
        info!("Cleaning up state manager...");
        let mut states = self.states.write().await;
        states.clear();
    }
}
