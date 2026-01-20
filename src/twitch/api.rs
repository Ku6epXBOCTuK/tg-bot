use crate::config::Config;
use reqwest::{Client, Error as ReqwestError, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[derive(Deserialize, Debug)]
pub struct TwitchStreamData {
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub started_at: String,
}

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

        // Get new token with retry logic
        let max_retries = self.config.max_retries;
        let base_delay = Duration::from_secs(self.config.retry_delay_seconds);

        for attempt in 1..=max_retries {
            let response = self
                .client
                .post("https://id.twitch.tv/oauth2/token")
                .query(&[
                    ("client_id", &self.config.twitch_client_id),
                    ("client_secret", &self.config.twitch_client_secret),
                    ("grant_type", &"client_credentials".to_string()),
                ])
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    // Handle rate limiting (HTTP 429)
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        if attempt < max_retries {
                            let retry_after = resp
                                .headers()
                                .get("Retry-After")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(60);

                            warn!(
                                "Rate limited by Twitch API (attempt {}/{}), waiting {} seconds",
                                attempt, max_retries, retry_after
                            );
                            sleep(Duration::from_secs(retry_after)).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(
                                "Rate limited by Twitch API, max retries exceeded".to_string(),
                            ));
                        }
                    }

                    if !status.is_success() {
                        if attempt < max_retries {
                            let delay = base_delay * 2_u32.pow(attempt - 1);
                            warn!(
                                "Failed to get access token (attempt {}/{}), retrying in {:?}",
                                attempt, max_retries, delay
                            );
                            sleep(delay).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(format!(
                                "Failed to get access token after {} attempts: {}",
                                max_retries, status
                            )));
                        }
                    }

                    #[derive(serde::Deserialize)]
                    struct TokenResponse {
                        access_token: String,
                        expires_in: i64,
                    }

                    let token_response: TokenResponse = resp.json().await?;
                    self.access_token = Some(token_response.access_token);
                    self.token_expires_at = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_secs(token_response.expires_in as u64 - 60), // 60 seconds buffer
                    );

                    info!("Successfully obtained Twitch access token");
                    return Ok(());
                }
                Err(e) => {
                    if attempt < max_retries {
                        let delay = base_delay * 2_u32.pow(attempt - 1);
                        warn!(
                            "Network error getting access token (attempt {}/{}), retrying in {:?}: {}",
                            attempt, max_retries, delay, e
                        );
                        sleep(delay).await;
                    } else {
                        return Err(TwitchApiError::Http(e));
                    }
                }
            }
        }

        Err(TwitchApiError::Api(
            "Failed to get access token after all retries".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub async fn get_user_id(&mut self, login: &str) -> Result<String, TwitchApiError> {
        self.ensure_access_token().await?;

        let max_retries = self.config.max_retries;
        let base_delay = Duration::from_secs(self.config.retry_delay_seconds);

        for attempt in 1..=max_retries {
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
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    // Handle rate limiting (HTTP 429)
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        if attempt < max_retries {
                            let retry_after = resp
                                .headers()
                                .get("Retry-After")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(60);

                            warn!(
                                "Rate limited by Twitch API (attempt {}/{}), waiting {} seconds",
                                attempt, max_retries, retry_after
                            );
                            sleep(Duration::from_secs(retry_after)).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(
                                "Rate limited by Twitch API, max retries exceeded".to_string(),
                            ));
                        }
                    }

                    if !status.is_success() {
                        if attempt < max_retries {
                            let delay = base_delay * 2_u32.pow(attempt - 1);
                            warn!(
                                "Failed to get user (attempt {}/{}), retrying in {:?}",
                                attempt, max_retries, delay
                            );
                            sleep(delay).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(format!(
                                "Failed to get user after {} attempts: {}",
                                max_retries, status
                            )));
                        }
                    }

                    #[derive(Deserialize)]
                    struct TwitchUserResponse {
                        data: Vec<TwitchUserData>,
                    }

                    #[derive(Deserialize)]
                    struct TwitchUserData {
                        id: String,
                        login: String,
                        display_name: String,
                        r#type: String,
                        broadcaster_type: String,
                        description: String,
                        profile_image_url: String,
                        offline_image_url: String,
                        view_count: i64,
                        created_at: String,
                    }

                    let user_response: TwitchUserResponse = resp.json().await?;

                    if user_response.data.is_empty() {
                        return Err(TwitchApiError::UserNotFound(login.to_string()));
                    }

                    return Ok(user_response.data[0].id.clone());
                }
                Err(e) => {
                    if attempt < max_retries {
                        let delay = base_delay * 2_u32.pow(attempt - 1);
                        warn!(
                            "Network error getting user (attempt {}/{}), retrying in {:?}: {}",
                            attempt, max_retries, delay, e
                        );
                        sleep(delay).await;
                    } else {
                        return Err(TwitchApiError::Http(e));
                    }
                }
            }
        }

        Err(TwitchApiError::Api(
            "Failed to get user after all retries".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub async fn get_stream_info(
        &mut self,
        user_id: &str,
    ) -> Result<Option<String>, TwitchApiError> {
        self.ensure_access_token().await?;

        let max_retries = self.config.max_retries;
        let base_delay = Duration::from_secs(self.config.retry_delay_seconds);

        for attempt in 1..=max_retries {
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
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    // Handle rate limiting (HTTP 429)
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        if attempt < max_retries {
                            let retry_after = resp
                                .headers()
                                .get("Retry-After")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(60);

                            warn!(
                                "Rate limited by Twitch API (attempt {}/{}), waiting {} seconds",
                                attempt, max_retries, retry_after
                            );
                            sleep(Duration::from_secs(retry_after)).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(
                                "Rate limited by Twitch API, max retries exceeded".to_string(),
                            ));
                        }
                    }

                    if !status.is_success() {
                        if attempt < max_retries {
                            let delay = base_delay * 2_u32.pow(attempt - 1);
                            warn!(
                                "Failed to get stream info (attempt {}/{}), retrying in {:?}",
                                attempt, max_retries, delay
                            );
                            sleep(delay).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(format!(
                                "Failed to get stream info after {} attempts: {}",
                                max_retries, status
                            )));
                        }
                    }

                    #[derive(Deserialize)]
                    struct TwitchStreamResponse {
                        data: Vec<TwitchStreamData>,
                    }

                    let stream_response: TwitchStreamResponse = resp.json().await?;

                    if stream_response.data.is_empty() {
                        return Ok(None);
                    }

                    return Ok(Some(stream_response.data[0].started_at.clone()));
                }
                Err(e) => {
                    if attempt < max_retries {
                        let delay = base_delay * 2_u32.pow(attempt - 1);
                        warn!(
                            "Network error getting stream info (attempt {}/{}), retrying in {:?}: {}",
                            attempt, max_retries, delay, e
                        );
                        sleep(delay).await;
                    } else {
                        return Err(TwitchApiError::Http(e));
                    }
                }
            }
        }

        Err(TwitchApiError::Api(
            "Failed to get stream info after all retries".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub async fn get_streams(
        &mut self,
        user_ids: &[String],
    ) -> Result<Vec<TwitchStreamData>, TwitchApiError> {
        self.ensure_access_token().await?;

        let mut query = Vec::new();
        for user_id in user_ids {
            query.push(("user_id", user_id.as_str()));
        }

        let max_retries = self.config.max_retries;
        let base_delay = Duration::from_secs(self.config.retry_delay_seconds);

        for attempt in 1..=max_retries {
            let response = self
                .client
                .get("https://api.twitch.tv/helix/streams")
                .header("Client-ID", &self.config.twitch_client_id)
                .header(
                    "Authorization",
                    format!("Bearer {}", self.access_token.as_ref().unwrap()),
                )
                .query(&query)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    // Handle rate limiting (HTTP 429)
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        if attempt < max_retries {
                            let retry_after = resp
                                .headers()
                                .get("Retry-After")
                                .and_then(|v| v.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(60);

                            warn!(
                                "Rate limited by Twitch API (attempt {}/{}), waiting {} seconds",
                                attempt, max_retries, retry_after
                            );
                            sleep(Duration::from_secs(retry_after)).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(
                                "Rate limited by Twitch API, max retries exceeded".to_string(),
                            ));
                        }
                    }

                    if !status.is_success() {
                        if attempt < max_retries {
                            let delay = base_delay * 2_u32.pow(attempt - 1);
                            warn!(
                                "Failed to get streams (attempt {}/{}), retrying in {:?}",
                                attempt, max_retries, delay
                            );
                            sleep(delay).await;
                            continue;
                        } else {
                            return Err(TwitchApiError::Api(format!(
                                "Failed to get streams after {} attempts: {}",
                                max_retries, status
                            )));
                        }
                    }

                    #[derive(Deserialize)]
                    struct TwitchStreamResponse {
                        data: Vec<TwitchStreamData>,
                    }

                    let stream_response: TwitchStreamResponse = resp.json().await?;
                    return Ok(stream_response.data);
                }
                Err(e) => {
                    if attempt < max_retries {
                        let delay = base_delay * 2_u32.pow(attempt - 1);
                        warn!(
                            "Network error getting streams (attempt {}/{}), retrying in {:?}: {}",
                            attempt, max_retries, delay, e
                        );
                        sleep(delay).await;
                    } else {
                        return Err(TwitchApiError::Http(e));
                    }
                }
            }
        }

        Err(TwitchApiError::Api(
            "Failed to get streams after all retries".to_string(),
        ))
    }
}
