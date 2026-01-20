use crate::state::models::{StreamState, StreamStatus};
use crate::storage::sqlite::Storage;
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
    #[allow(dead_code)]
    storage: Storage,
    grace_period_online: u64,
    grace_period_offline: u64,
}

#[derive(Clone)]
pub struct StreamEvent {
    pub streamer_id: String,
    pub streamer_login: String,
    pub streamer_name: String,
    pub event_id: String,
    pub event_timestamp: String,
    pub started_at: String,
    pub target_chat_ids: Vec<String>,
}

impl StateManager {
    pub fn new(
        telegram_bot: TelegramBot,
        storage: Storage,
        grace_period_online: u64,
        grace_period_offline: u64,
    ) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            telegram_bot: Arc::new(Mutex::new(telegram_bot)),
            storage,
            grace_period_online,
            grace_period_offline,
        }
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

    pub async fn handle_stream_online(&self, event: StreamEvent) {
        info!(
            "Handling stream.online for {} (ID: {})",
            event.streamer_login, event.streamer_id
        );

        self.ensure_state(
            &event.streamer_id,
            &event.streamer_login,
            &event.streamer_name,
        )
        .await;

        let mut states = self.states.write().await;
        let state = states.get_mut(&event.streamer_id).unwrap();

        // Check for duplicate events
        if state.is_duplicate_event(&event.event_id, &event.event_timestamp) {
            info!(
                "Duplicate stream.online event detected for {}",
                event.streamer_login
            );
            return;
        }

        state.update_event_info(event.event_id.clone(), &event.event_timestamp);

        match state.status {
            StreamStatus::Offline => {
                self.handle_offline_to_online(
                    state,
                    &event.streamer_login,
                    &event.started_at,
                    event.target_chat_ids,
                )
                .await;
            }
            StreamStatus::OnlinePending => {
                self.handle_online_pending_duplicate(&event.streamer_login)
                    .await;
            }
            StreamStatus::Online => {
                self.handle_online_duplicate(&event.streamer_login).await;
            }
            StreamStatus::OfflinePending => {
                self.handle_offline_pending_to_online(state, &event.streamer_login)
                    .await;
            }
        }
    }

    async fn handle_online_pending_duplicate(&self, streamer_login: &str) {
        info!(
            "Stream {} already in OnlinePending state, ignoring",
            streamer_login
        );
    }

    async fn handle_online_duplicate(&self, streamer_login: &str) {
        info!("Stream {} already online, ignoring", streamer_login);
    }

    async fn handle_offline_pending_to_online(
        &self,
        state: &mut StreamState,
        streamer_login: &str,
    ) {
        info!(
            "Stream {} was OfflinePending, cancelling offline timer",
            streamer_login
        );
        state.status = StreamStatus::Online;
        state.grace_period_start = None;
    }

    async fn handle_offline_to_online(
        &self,
        state: &mut StreamState,
        streamer_login: &str,
        started_at: &str,
        target_chat_ids: Vec<String>,
    ) {
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
        let streamer_id_clone = state.streamer_id.clone();
        let streamer_login_clone = streamer_login.to_string();
        let grace_period_start = state.grace_period_start;
        let target_chat_ids_clone = target_chat_ids.clone();

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
                    state_manager
                        .confirm_online(state, &streamer_login_clone, target_chat_ids_clone)
                        .await;
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

    async fn confirm_online(
        &self,
        state: &mut StreamState,
        streamer_login: &str,
        target_chat_ids: Vec<String>,
    ) {
        info!(
            "Grace period ended for {}, confirming online",
            streamer_login
        );

        // Check if target_chat_id is configured
        if target_chat_ids.is_empty() {
            error!(
                "No target chat configured for streamer {}, transitioning to Online without notification",
                streamer_login
            );
            state.status = StreamStatus::Online;
            state.started_at = state.pending_started_at;
            state.pending_started_at = None;
            state.grace_period_start = None;
            return;
        }

        // Send notification to all configured chats
        let telegram_bot = self.telegram_bot.lock().await;
        if let Some(_started_at) = state.pending_started_at {
            let mut message_id: Option<i32> = None;
            for chat_id_str in &target_chat_ids {
                let chat_id: i64 = match chat_id_str.parse() {
                    Ok(id) => id,
                    Err(e) => {
                        error!("Failed to parse chat_id '{}': {}", chat_id_str, e);
                        continue;
                    }
                };

                if chat_id == 0 {
                    error!(
                        "Invalid chat_id 0 for streamer {}, skipping notification",
                        streamer_login
                    );
                    continue;
                }

                if let Some(id) = telegram_bot
                    .send_stream_notification(
                        chat_id,
                        &state.streamer_login,
                        "🔴 <b>Стрим начался!</b>",
                        None,
                    )
                    .await
                {
                    // Store the first successful message ID
                    if message_id.is_none() {
                        message_id = Some(id);
                    }
                    info!(
                        "Sent notification to chat {} for streamer {} (message_id: {})",
                        chat_id, streamer_login, id
                    );
                } else {
                    error!(
                        "Failed to send Telegram notification to chat {} for {}",
                        chat_id, streamer_login
                    );
                }
            }

            if let Some(id) = message_id {
                state.telegram_message_id = Some(id);
                state.started_at = state.pending_started_at;
                state.status = StreamStatus::Online;
                state.pending_started_at = None;
                state.grace_period_start = None;

                info!(
                    "Stream {} confirmed online, message ID: {}",
                    streamer_login, id
                );
            } else {
                error!(
                    "Failed to send Telegram notification to any chat for {}",
                    streamer_login
                );
                state.status = StreamStatus::Online;
                state.started_at = state.pending_started_at;
                state.pending_started_at = None;
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
                self.handle_online_to_offline(state, streamer_login).await;
            }
            StreamStatus::OfflinePending => {
                self.handle_offline_pending_duplicate(streamer_login).await;
            }
            StreamStatus::Offline => {
                self.handle_offline_duplicate(streamer_login).await;
            }
            StreamStatus::OnlinePending => {
                self.handle_online_pending_to_offline(state, streamer_login)
                    .await;
            }
        }
    }

    async fn handle_offline_pending_duplicate(&self, streamer_login: &str) {
        info!(
            "Stream {} already in OfflinePending state, ignoring",
            streamer_login
        );
    }

    async fn handle_offline_duplicate(&self, streamer_login: &str) {
        info!("Stream {} already offline, ignoring", streamer_login);
    }

    async fn handle_online_pending_to_offline(
        &self,
        state: &mut StreamState,
        streamer_login: &str,
    ) {
        info!(
            "Stream {} was OnlinePending, cancelling online timer",
            streamer_login
        );
        state.status = StreamStatus::Offline;
        state.pending_started_at = None;
        state.grace_period_start = None;
    }

    async fn handle_online_to_offline(&self, state: &mut StreamState, streamer_login: &str) {
        // Start grace period for offline
        state.status = StreamStatus::OfflinePending;
        state.grace_period_start = Some(Utc::now());

        info!(
            "Stream {} entered OfflinePending state, grace period: {}s",
            streamer_login, self.grace_period_offline
        );

        let state_manager = self.clone();
        let streamer_id_clone = state.streamer_id.clone();
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
                    state_manager
                        .confirm_offline(state, &streamer_login_clone)
                        .await;
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

    async fn confirm_offline(&self, state: &mut StreamState, streamer_login: &str) {
        info!(
            "Grace period ended for {}, confirming offline",
            streamer_login
        );

        // Delete notification
        // Note: Message deletion is not currently supported in the per-user/channel architecture
        // Each user has their own notification settings, so we can't determine which chat to delete from
        // This is a known limitation of the current design
        if let Some(_message_id) = state.telegram_message_id {
            info!(
                "Stream {} offline, but message deletion is not supported in per-user/channel mode",
                streamer_login
            );
        }

        // Reset state
        state.status = StreamStatus::Offline;
        state.started_at = None;
        state.telegram_message_id = None;
        state.grace_period_start = None;

        info!("Stream {} confirmed offline", streamer_login);
    }
}
