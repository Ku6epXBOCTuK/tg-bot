use crate::storage::sqlite::Storage;
use crate::telegram::bot::TelegramBot;
use crate::telegram::commands::{Command, CommandError};
use crate::twitch::api::TwitchApiClient;
use tracing::info;

pub struct CommandHandler {
    storage: Storage,
    bot: TelegramBot,
    twitch_client: TwitchApiClient,
}

impl CommandHandler {
    pub fn new(storage: Storage, bot: TelegramBot, twitch_client: TwitchApiClient) -> Self {
        Self {
            storage,
            bot,
            twitch_client,
        }
    }

    pub async fn handle_command(
        &self,
        command: Command,
        user_id: i64,
    ) -> Result<String, CommandError> {
        match command {
            Command::Start => Ok(self.handle_start()),
            Command::Add(streamer_login) => self.handle_add(user_id, &streamer_login).await,
            Command::MySettings => self.handle_mysettings(user_id).await,
            Command::SetChannel(channel) => self.handle_set_channel(user_id, &channel).await,
            Command::SetText(text) => self.handle_set_text(user_id, &text).await,
            Command::AddButton(button) => self.handle_add_button(user_id, &button).await,
            Command::ClearButtons => self.handle_clear_buttons(user_id).await,
            Command::Test => self.handle_test(user_id).await,
            Command::Preview => self.handle_preview(user_id).await,
            Command::Remove(streamer_login) => self.handle_remove(user_id, &streamer_login).await,
            Command::Help => Ok(self.handle_help()),
        }
    }

    async fn handle_add(&self, user_id: i64, streamer_login: &str) -> Result<String, CommandError> {
        // Get Twitch user ID
        let mut client = self.twitch_client.clone();
        let twitch_user_id = client
            .get_user_id(streamer_login)
            .await
            .map_err(|e| CommandError::TwitchApiError(e.to_string()))?;

        // Add subscription
        let subscription_id = self
            .storage
            .add_user_subscription(user_id, &twitch_user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        // Save default settings - channel is empty (not set)
        self.storage
            .save_notification_settings(subscription_id, "", "🔴 <b>Стрим начался!</b>", None)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        info!(
            "User {} added streamer {} ({})",
            user_id, streamer_login, twitch_user_id
        );

        Ok(format!(
            "✅ <b>Добавлен стример:</b> {} (@{})\n\n\
            Используйте /set_channel для указания чата для уведомлений.\n\
            Используйте /set_text для кастомного текста.\n\
            Используйте /add_button для добавления кнопок.",
            streamer_login, streamer_login
        ))
    }

    async fn handle_mysettings(&self, user_id: i64) -> Result<String, CommandError> {
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Настройки не установлены. Используйте /add username".to_string());
        }

        let mut response = format!("📋 <b>Ваши подписки и настройки:</b>\n\n");

        for (index, sub) in subscriptions.iter().enumerate() {
            let settings = self
                .storage
                .get_notification_settings(sub.id)
                .await
                .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

            if let Some(settings) = settings {
                response.push_str(&format!(
                    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
                    <b>Подписка #{}:</b> <code>{}</code>\n\n\
                    📺 <b>Стример:</b> @{}\n\
                    💬 <b>Чат для уведомлений:</b> <code>{}</code>\n\
                    📝 <b>Текст уведомления:</b> {}\n",
                    index + 1,
                    sub.id,
                    sub.twitch_user_id,
                    settings.target_chat_id,
                    settings.custom_message
                ));

                // Parse and display buttons
                if let Some(json) = &settings.inline_buttons_json {
                    if let Ok(buttons) = serde_json::from_str::<Vec<(String, String)>>(json) {
                        if !buttons.is_empty() {
                            response.push_str("🔘 <b>Кнопки:</b>\n");
                            for (btn_index, (btn_text, btn_url)) in buttons.iter().enumerate() {
                                response.push_str(&format!(
                                    "  {}. {} | <code>{}</code>\n",
                                    btn_index + 1,
                                    btn_text,
                                    btn_url
                                ));
                            }
                        } else {
                            response.push_str("🔘 <b>Кнопки:</b> нет\n");
                        }
                    } else {
                        response.push_str("🔘 <b>Кнопки:</b> ❌ ошибка парсинга\n");
                    }
                } else {
                    response.push_str("🔘 <b>Кнопки:</b> нет\n");
                }

                response.push_str("\n");
            }
        }

        response.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n");
        response.push_str("💡 <i>Используйте команды для настройки:</i>\n");
        response.push_str("• /set_channel - изменить чат\n");
        response.push_str("• /set_text - изменить текст\n");
        response.push_str("• /add_button - добавить кнопку\n");
        response.push_str("• /clear_buttons - удалить все кнопки");

        Ok(response)
    }

    async fn handle_set_channel(
        &self,
        user_id: i64,
        channel: &str,
    ) -> Result<String, CommandError> {
        // Validate channel is not empty
        if channel.trim().is_empty() {
            return Err(CommandError::InvalidFormat(
                "❌ Укажите чат для уведомлений. Формат: /set_channel @username или ID".to_string(),
            ));
        }

        // Parse and validate channel ID
        let chat_id = if channel.starts_with('@') {
            // It's a username - validate format
            let username = &channel[1..]; // Remove @
            if username.is_empty() {
                return Err(CommandError::InvalidFormat(
                    "❌ Укажите username после @. Формат: /set_channel @username".to_string(),
                ));
            }
            if username.contains(' ') {
                return Err(CommandError::InvalidFormat(
                    "❌ Username не должен содержать пробелов".to_string(),
                ));
            }
            channel.to_string()
        } else {
            // It's a numeric ID - validate
            match channel.parse::<i64>() {
                Ok(id) => {
                    if id == 0 {
                        return Err(CommandError::InvalidFormat(
                            "❌ ID чата не может быть 0".to_string(),
                        ));
                    }
                    id.to_string()
                }
                Err(_) => {
                    return Err(CommandError::InvalidFormat(
                        "❌ Неверный формат ID чата. Укажите числовой ID или username с @"
                            .to_string(),
                    ));
                }
            }
        };

        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Update settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if let Some(settings) = settings {
            self.storage
                .save_notification_settings(
                    last_sub.id,
                    &chat_id,
                    &settings.custom_message,
                    settings.inline_buttons_json.as_deref(),
                )
                .await
                .map_err(|e| CommandError::DatabaseError(e.to_string()))?;
        }

        Ok(format!("✅ Чат для уведомлений установлен: {}", chat_id))
    }

    async fn handle_set_text(&self, user_id: i64, text: &str) -> Result<String, CommandError> {
        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Update settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if let Some(settings) = settings {
            self.storage
                .save_notification_settings(
                    last_sub.id,
                    &settings.target_chat_id,
                    text,
                    settings.inline_buttons_json.as_deref(),
                )
                .await
                .map_err(|e| CommandError::DatabaseError(e.to_string()))?;
        }

        Ok(format!("✅ Текст уведомления установлен: {}", text))
    }

    async fn handle_add_button(&self, user_id: i64, button: &str) -> Result<String, CommandError> {
        let parts: Vec<&str> = button.split('|').collect();
        if parts.len() != 2 {
            return Err(CommandError::InvalidFormat(
                "Формат: /add_button Текст | URL".to_string(),
            ));
        }

        let text = parts[0].trim();
        let url = parts[1].trim();

        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Update settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        let mut buttons = Vec::new();
        if let Some(settings) = &settings {
            if let Some(existing) = &settings.inline_buttons_json {
                if let Ok(existing_buttons) =
                    serde_json::from_str::<Vec<(String, String)>>(existing)
                {
                    buttons = existing_buttons;
                }
            }
        }

        buttons.push((text.to_string(), url.to_string()));

        let buttons_json = serde_json::to_string(&buttons).unwrap();

        self.storage
            .save_notification_settings(
                last_sub.id,
                &settings
                    .as_ref()
                    .map(|s| s.target_chat_id.clone())
                    .unwrap_or("".to_string()),
                &settings
                    .as_ref()
                    .map(|s| s.custom_message.clone())
                    .unwrap_or("🔴 <b>Стрим начался!</b>".to_string()),
                Some(&buttons_json),
            )
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        Ok(format!("✅ Кнопка добавлена: {} | {}", text, url))
    }

    async fn handle_clear_buttons(&self, user_id: i64) -> Result<String, CommandError> {
        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Update settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if let Some(settings) = settings {
            self.storage
                .save_notification_settings(
                    last_sub.id,
                    &settings.target_chat_id,
                    &settings.custom_message,
                    None,
                )
                .await
                .map_err(|e| CommandError::DatabaseError(e.to_string()))?;
        }

        Ok("✅ Все кнопки удалены".to_string())
    }

    async fn handle_test(&self, user_id: i64) -> Result<String, CommandError> {
        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Get settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if let Some(settings) = settings {
            // Check if target chat is configured
            if settings.target_chat_id.is_empty() {
                return Ok(
                    "❌ Чат для уведомлений не настроен. Используйте /set_channel".to_string(),
                );
            }

            // Check if target_chat_id is a numeric ID or a username
            let chat_id: i64 = if let Ok(id) = settings.target_chat_id.parse::<i64>() {
                id
            } else {
                // If it's a username (starts with @), we can't send to it directly
                return Ok("❌ Чат для уведомлений настроен как username (@channel). Для теста укажите числовой ID чата с помощью /set_channel".to_string());
            };

            // Parse inline buttons
            let inline_buttons = if let Some(json) = settings.inline_buttons_json {
                if let Ok(buttons) = serde_json::from_str::<Vec<(String, String)>>(&json) {
                    Some(buttons)
                } else {
                    None
                }
            } else {
                None
            };

            // Send test notification to configured channel
            self.bot
                .send_stream_notification(
                    chat_id,
                    &last_sub.twitch_user_id,
                    &settings.custom_message,
                    inline_buttons,
                )
                .await;

            Ok("✅ Тестовое уведомление отправлено в настроенный чат".to_string())
        } else {
            Ok("❌ Настройки не найдены".to_string())
        }
    }

    async fn handle_preview(&self, user_id: i64) -> Result<String, CommandError> {
        // Get the last subscription
        let subscriptions = self
            .storage
            .get_user_subscriptions(user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if subscriptions.is_empty() {
            return Ok("❌ Сначала добавьте стримера с помощью /add".to_string());
        }

        let last_sub = subscriptions.last().unwrap();

        // Get settings
        let settings = self
            .storage
            .get_notification_settings(last_sub.id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        if let Some(settings) = settings {
            // Always send preview to user's private chat
            let chat_id: i64 = user_id;

            // Parse inline buttons
            let inline_buttons = if let Some(json) = settings.inline_buttons_json {
                if let Ok(buttons) = serde_json::from_str::<Vec<(String, String)>>(&json) {
                    Some(buttons)
                } else {
                    None
                }
            } else {
                None
            };

            // Send preview to private chat
            self.bot
                .send_stream_notification(
                    chat_id,
                    &last_sub.twitch_user_id,
                    &settings.custom_message,
                    inline_buttons,
                )
                .await;

            Ok("✅ Превью уведомления отправлено в личные сообщения".to_string())
        } else {
            Ok("❌ Настройки не найдены".to_string())
        }
    }

    async fn handle_remove(
        &self,
        user_id: i64,
        streamer_login: &str,
    ) -> Result<String, CommandError> {
        // Get Twitch user ID
        let mut client = self.twitch_client.clone();
        let twitch_user_id = client
            .get_user_id(streamer_login)
            .await
            .map_err(|e| CommandError::TwitchApiError(e.to_string()))?;

        // Delete subscription
        self.storage
            .delete_user_subscription(user_id, &twitch_user_id)
            .await
            .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

        info!(
            "User {} removed streamer {} ({})",
            user_id, streamer_login, twitch_user_id
        );

        Ok(format!(
            "✅ Стример {} удален из вашего списка",
            streamer_login
        ))
    }

    fn handle_start(&self) -> String {
        r#"👋 <b>Привет!</b>

Я бот для отслеживания стримов на Twitch. Я буду уведомлять вас, когда стример начинает или заканчивает стрим.

<b>Как использовать:</b>
1. Добавьте стримера: /add username
2. Настройте чат для уведомлений: /set_channel @username или ID
3. Настройте текст уведомления: /set_text текст
4. Добавьте кнопки: /add_button текст | url
5. Отправьте тестовое уведомление: /test
6. Превью уведомления: /preview

<b>Другие команды:</b>
/my_settings - Показать ваши подписки и настройки
/clear_buttons - Удалить все кнопки
/remove username - Удалить стримера
/help - Показать эту справку

<b>Примеры:</b>
/add streamer123
/set_channel @my_channel
/set_text 🔴 <b>Стрим начался!</b>
/add_button Перейти на стрим | https://twitch.tv/streamer123"#
            .to_string()
    }

    pub fn handle_help(&self) -> String {
        r#"🤖 <b>Доступные команды:</b>

/start - Начать работу с ботом и увидеть приветствие
/add username - Добавить стримера
/my_settings - Показать ваши подписки
/set_channel @username или ID - Установить чат для уведомлений
/set_text текст - Установить кастомный текст
/add_button текст | url - Добавить кнопку
/clear_buttons - Удалить все кнопки
/test - Отправить тестовое уведомление в настроенный чат
/preview - Превью уведомления в личные сообщения
/remove username - Удалить стримера
/help - Показать эту справку"#
            .to_string()
    }
}
