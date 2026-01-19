use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubChallenge {
    pub challenge: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubNotification {
    pub subscription: EventSubSubscription,
    pub event: EventSubEvent,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubSubscription {
    pub id: String,
    pub status: String,
    #[serde(rename = "type")]
    pub sub_type: String,
    pub version: String,
    pub cost: i64,
    pub condition: HashMap<String, String>,
    pub created_at: String,
    pub transport: EventSubTransport,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubTransport {
    pub method: String,
    pub callback: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum EventSubEvent {
    #[serde(rename = "stream.online")]
    StreamOnline(StreamOnlineEvent),
    #[serde(rename = "stream.offline")]
    StreamOffline(StreamOfflineEvent),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StreamOnlineEvent {
    pub id: String,
    pub broadcaster_user_id: String,
    pub broadcaster_user_login: String,
    pub broadcaster_user_name: String,
    pub r#type: String, // "live", "playlist", "watch_party", "rerun"
    pub started_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StreamOfflineEvent {
    pub broadcaster_user_id: String,
    pub broadcaster_user_login: String,
    pub broadcaster_user_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubSubscriptionRequest {
    pub r#type: String,
    pub version: String,
    pub condition: HashMap<String, String>,
    pub transport: EventSubTransport,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubSubscriptionResponse {
    pub data: Vec<EventSubSubscriptionData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventSubSubscriptionData {
    pub id: String,
    pub status: String,
    #[serde(rename = "type")]
    pub sub_type: String,
    pub version: String,
    pub cost: i64,
    pub condition: HashMap<String, String>,
    pub created_at: String,
    pub transport: EventSubTransport,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitchUserResponse {
    pub data: Vec<TwitchUserData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitchUserData {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub r#type: String,
    pub broadcaster_type: String,
    pub description: String,
    pub profile_image_url: String,
    pub offline_image_url: String,
    pub view_count: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitchStreamResponse {
    pub data: Vec<TwitchStreamData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitchStreamData {
    pub id: String,
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub game_id: String,
    pub game_name: String,
    pub r#type: String,
    pub title: String,
    pub viewer_count: i64,
    pub started_at: String,
    pub language: String,
    pub thumbnail_url: String,
    pub tag_ids: Vec<String>,
    pub is_mature: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TwitchEventSubHeaders {
    pub message_id: String,
    pub message_timestamp: String,
    pub message_type: String,
    pub subscription_type: String,
    pub subscription_version: String,
    pub signature: String,
}

impl TwitchEventSubHeaders {
    pub fn from_headers(headers: &HashMap<String, String>) -> Option<Self> {
        let message_id = headers.get("Twitch-Eventsub-Message-Id")?.clone();
        let message_timestamp = headers.get("Twitch-Eventsub-Message-Timestamp")?.clone();
        let message_type = headers.get("Twitch-Eventsub-Message-Type")?.clone();
        let subscription_type = headers.get("Twitch-Eventsub-Subscription-Type")?.clone();
        let subscription_version = headers.get("Twitch-Eventsub-Subscription-Version")?.clone();
        let signature = headers.get("Twitch-Eventsub-Message-Signature")?.clone();

        Some(Self {
            message_id,
            message_timestamp,
            message_type,
            subscription_type,
            subscription_version,
            signature,
        })
    }
}
