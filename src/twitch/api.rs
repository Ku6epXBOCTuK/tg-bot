use crate::config::Config;
use crate::twitch::eventsub::{
    EventSubSubscriptionRequest, EventSubSubscriptionResponse, TwitchStreamResponse,
    TwitchUserResponse,
};
use reqwest::{Client, Error as ReqwestError};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum TwitchApiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] ReqwestError),
    #[error("API error: {0}")]
    Api(String),
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Stream not found: {0}")]
    StreamNotFound(String),
}

#[derive(Clone)]
pub struct TwitchApiClient {
    client: Client,
    config: Config,
    access_token: Option<String>,
    token_expires_at: Option<std::time::Instant>,
}

impl TwitchApiClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
            access_token: None,
            token_expires_at: None,
        }
    }

    #[allow(dead_code)]
    async fn ensure_access_token(&mut self) -> Result<(), TwitchApiError> {
        // Check if token is still valid
        if let (Some(_token), Some(expires_at)) = (&self.access_token, self.token_expires_at) {
            if std::time::Instant::now() < expires_at {
                return Ok(());
            }
            info!("Twitch access token expired, refreshing...");
        }

        // Get new token
        let response = self
            .client
            .post("https://id.twitch.tv/oauth2/token")
            .query(&[
                ("client_id", &self.config.twitch_client_id),
                ("client_secret", &self.config.twitch_client_secret),
                ("grant_type", &"client_credentials".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TwitchApiError::Api(format!(
                "Failed to get access token: {}",
                response.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: i64,
        }

        let token_response: TokenResponse = response.json().await?;
        self.access_token = Some(token_response.access_token);
        self.token_expires_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_secs(token_response.expires_in as u64 - 60), // 60 seconds buffer
        );

        info!("Successfully obtained Twitch access token");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_user_id(&mut self, login: &str) -> Result<String, TwitchApiError> {
        self.ensure_access_token().await?;

        let response = self
            .client
            .get("https://api.twitch.tv/helix/users")
            .header("Client-ID", &self.config.twitch_client_id)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.as_ref().unwrap()),
            )
            .query(&[("login", login)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TwitchApiError::Api(format!(
                "Failed to get user: {}",
                response.status()
            )));
        }

        let user_response: TwitchUserResponse = response.json().await?;

        if user_response.data.is_empty() {
            return Err(TwitchApiError::UserNotFound(login.to_string()));
        }

        Ok(user_response.data[0].id.clone())
    }

    #[allow(dead_code)]
    pub async fn get_stream_info(
        &mut self,
        user_id: &str,
    ) -> Result<Option<String>, TwitchApiError> {
        self.ensure_access_token().await?;

        let response = self
            .client
            .get("https://api.twitch.tv/helix/streams")
            .header("Client-ID", &self.config.twitch_client_id)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.as_ref().unwrap()),
            )
            .query(&[("user_id", user_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TwitchApiError::Api(format!(
                "Failed to get stream info: {}",
                response.status()
            )));
        }

        let stream_response: TwitchStreamResponse = response.json().await?;

        if stream_response.data.is_empty() {
            return Ok(None);
        }

        Ok(Some(stream_response.data[0].started_at.clone()))
    }

    #[allow(dead_code)]
    pub async fn create_eventsub_subscription(
        &mut self,
        user_id: &str,
        subscription_type: &str,
    ) -> Result<String, TwitchApiError> {
        self.ensure_access_token().await?;

        let request = EventSubSubscriptionRequest {
            r#type: subscription_type.to_string(),
            version: "1".to_string(),
            condition: {
                let mut map = std::collections::HashMap::new();
                map.insert("broadcaster_user_id".to_string(), user_id.to_string());
                map
            },
            transport: crate::twitch::eventsub::EventSubTransport {
                method: "webhook".to_string(),
                callback: self.config.twitch_webhook_url(),
            },
        };

        let response = self
            .client
            .post("https://api.twitch.tv/helix/eventsub/subscriptions")
            .header("Client-ID", &self.config.twitch_client_id)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.as_ref().unwrap()),
            )
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(TwitchApiError::Api(format!(
                "Failed to create subscription: {} - {}",
                status, error_text
            )));
        }

        let response_data: EventSubSubscriptionResponse = response.json().await?;

        if response_data.data.is_empty() {
            return Err(TwitchApiError::Api(
                "No subscription data returned".to_string(),
            ));
        }

        let subscription_id = response_data.data[0].id.clone();
        info!(
            "Created {} subscription for user_id {}: {}",
            subscription_type, user_id, subscription_id
        );

        Ok(subscription_id)
    }

    #[allow(dead_code)]
    pub async fn get_eventsub_subscriptions(
        &mut self,
    ) -> Result<Vec<crate::twitch::eventsub::EventSubSubscriptionData>, TwitchApiError> {
        self.ensure_access_token().await?;

        let response = self
            .client
            .get("https://api.twitch.tv/helix/eventsub/subscriptions")
            .header("Client-ID", &self.config.twitch_client_id)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.as_ref().unwrap()),
            )
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(TwitchApiError::Api(format!(
                "Failed to get subscriptions: {}",
                response.status()
            )));
        }

        let response_data: EventSubSubscriptionResponse = response.json().await?;
        Ok(response_data.data)
    }

    #[allow(dead_code)]
    pub async fn delete_eventsub_subscription(
        &mut self,
        subscription_id: &str,
    ) -> Result<(), TwitchApiError> {
        self.ensure_access_token().await?;

        let response = self
            .client
            .delete("https://api.twitch.tv/helix/eventsub/subscriptions")
            .header("Client-ID", &self.config.twitch_client_id)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.as_ref().unwrap()),
            )
            .query(&[("id", subscription_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            warn!(
                "Failed to delete subscription {}: {}",
                subscription_id,
                response.status()
            );
            return Err(TwitchApiError::Api(format!(
                "Failed to delete subscription: {}",
                response.status()
            )));
        }

        info!("Deleted EventSub subscription: {}", subscription_id);
        Ok(())
    }
}
