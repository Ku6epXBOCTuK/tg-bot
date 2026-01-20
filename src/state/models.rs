use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamStatus {
    Offline,
    OnlinePending,
    Online,
    OfflinePending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamState {
    pub streamer_id: String,
    pub streamer_login: String,
    pub streamer_name: String,
    pub status: StreamStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub pending_started_at: Option<DateTime<Utc>>,
    pub telegram_message_id: Option<i32>,
    pub last_event_id: Option<String>,
    pub last_event_timestamp: Option<DateTime<Utc>>,
    pub grace_period_start: Option<DateTime<Utc>>,
}

impl StreamState {
    pub fn new(streamer_id: String, streamer_login: String, streamer_name: String) -> Self {
        Self {
            streamer_id,
            streamer_login,
            streamer_name,
            status: StreamStatus::Offline,
            started_at: None,
            pending_started_at: None,
            telegram_message_id: None,
            last_event_id: None,
            last_event_timestamp: None,
            grace_period_start: None,
        }
    }

    pub fn is_duplicate_event(&self, event_id: &str, event_timestamp: &str) -> bool {
        if let Some(last_id) = &self.last_event_id {
            if last_id == event_id {
                return true;
            }
        }

        if let Some(last_ts) = self.last_event_timestamp {
            if let Ok(event_ts) = DateTime::parse_from_rfc3339(event_timestamp) {
                let event_ts_utc = event_ts.with_timezone(&Utc);
                // If event is older than 30 seconds, it's likely a duplicate
                if (event_ts_utc - last_ts).num_seconds().abs() < 30 {
                    return true;
                }
            }
        }

        false
    }

    pub fn update_event_info(&mut self, event_id: String, event_timestamp: &str) {
        self.last_event_id = Some(event_id);
        if let Ok(ts) = DateTime::parse_from_rfc3339(event_timestamp) {
            self.last_event_timestamp = Some(ts.with_timezone(&Utc));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamerConfig {
    pub streamer_id: String,
    pub streamer_login: String,
    pub streamer_name: String,
    pub created_at: DateTime<Utc>,
    pub eventsub_subscription_id: Option<String>,
    pub online_subscription_id: Option<String>,
    pub offline_subscription_id: Option<String>,
}
