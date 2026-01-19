use crate::config::Config;
use crate::state::manager::StateManager;
use crate::twitch::eventsub::{EventSubChallenge, EventSubNotification, TwitchEventSubHeaders};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub state_manager: Arc<StateManager>,
}

pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/webhook/twitch", get(handle_twitch_verification))
        .route("/webhook/twitch", post(handle_twitch_webhook))
        .with_state(app_state)
}

async fn handle_twitch_verification(
    State(_state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    info!("Received Twitch webhook verification request");

    // Extract challenge parameter from query string
    let challenge = headers
        .get("hub.challenge")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            // Try to parse from body if it's a challenge request
            if let Ok(challenge_req) = serde_json::from_str::<EventSubChallenge>(&body) {
                Some(challenge_req.challenge)
            } else {
                None
            }
        });

    if let Some(challenge) = challenge {
        info!("Responding to Twitch webhook verification with challenge");
        (StatusCode::OK, challenge)
    } else {
        warn!("No challenge found in verification request");
        (StatusCode::BAD_REQUEST, "Missing challenge".to_string())
    }
}

async fn handle_twitch_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    info!("Received Twitch webhook event");

    // Verify webhook signature
    if !verify_webhook_signature(&headers, &body, &state.config.webhook_secret) {
        error!("Webhook signature verification failed");
        return StatusCode::UNAUTHORIZED;
    }

    // Parse headers
    let mut header_map = HashMap::<String, String>::new();
    for (key, value) in headers.iter() {
        let key_str = key.to_string();
        if let Ok(value_str) = value.to_str() {
            header_map.insert(key_str, value_str.to_string());
        }
    }

    let event_headers = match TwitchEventSubHeaders::from_headers(&header_map) {
        Some(h) => h,
        None => {
            error!("Failed to parse Twitch event headers");
            return StatusCode::BAD_REQUEST;
        }
    };

    // Handle different message types
    match event_headers.message_type.as_str() {
        "webhook_callback_verification" => {
            // This should be handled by the GET endpoint, but just in case
            if let Ok(_challenge_req) = serde_json::from_str::<EventSubChallenge>(&body) {
                info!("Received verification challenge via POST");
                // Return OK, but don't send challenge back
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            }
        }
        "notification" => {
            // Process the actual event
            match serde_json::from_str::<EventSubNotification>(&body) {
                Ok(notification) => {
                    process_event(&state, notification, &event_headers).await;
                    StatusCode::OK
                }
                Err(e) => {
                    error!("Failed to parse notification: {}", e);
                    StatusCode::BAD_REQUEST
                }
            }
        }
        "revocation" => {
            info!("Received subscription revocation: {:?}", event_headers);
            StatusCode::OK
        }
        _ => {
            info!("Unknown message type: {}", event_headers.message_type);
            StatusCode::OK
        }
    }
}

fn verify_webhook_signature(headers: &HeaderMap, body: &str, secret: &str) -> bool {
    let signature_header = match headers.get("Twitch-Eventsub-Message-Signature") {
        Some(h) => match h.to_str() {
            Ok(s) => s,
            Err(_) => return false,
        },
        None => return false,
    };

    // Extract the signature value (format: "sha256=...")
    let signature_parts: Vec<&str> = signature_header.split('=').collect();
    if signature_parts.len() != 2 || signature_parts[0] != "sha256" {
        return false;
    }

    let expected_signature = signature_parts[1];

    // Calculate HMAC
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let result = mac.finalize();
    let computed_signature = hex::encode(result.into_bytes());

    computed_signature == expected_signature
}

async fn process_event(
    state: &AppState,
    notification: EventSubNotification,
    headers: &TwitchEventSubHeaders,
) {
    let event_id = &headers.message_id;
    let event_timestamp = &headers.message_timestamp;

    match notification.event {
        crate::twitch::eventsub::EventSubEvent::StreamOnline(event) => {
            info!(
                "Stream online event: {} (ID: {})",
                event.broadcaster_user_login, event.id
            );

            state
                .state_manager
                .handle_stream_online(
                    &event.broadcaster_user_id,
                    &event.broadcaster_user_login,
                    &event.broadcaster_user_name,
                    event_id,
                    event_timestamp,
                    &event.started_at,
                )
                .await;
        }
        crate::twitch::eventsub::EventSubEvent::StreamOffline(event) => {
            info!("Stream offline event: {}", event.broadcaster_user_login);

            state
                .state_manager
                .handle_stream_offline(
                    &event.broadcaster_user_id,
                    &event.broadcaster_user_login,
                    &event.broadcaster_user_name,
                    event_id,
                    event_timestamp,
                )
                .await;
        }
    }
}
