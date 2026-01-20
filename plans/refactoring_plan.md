# План рефакторинга кода

## 1. Дублирование кода отправки сообщений

**Проблема**: В `src/telegram/bot.rs` есть две функции с одинаковой логикой retry: `send_stream_notification()` (строки 55-127) и `send_message()` (строки 129-163).

**Решение**: Вынести общую логику в приватный метод `send_with_retry()`

### Шаги:

1. Создать приватный метод в `src/telegram/bot.rs`:

```rust
async fn send_with_retry<F, Fut>(&self, chat_id: i64, operation: F) -> Option<i32>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<teloxide::types::Message, teloxide::RequestError>>,
{
    for attempt in 1..=self.max_retries {
        match operation().await {
            Ok(msg) => {
                info!("Sent Telegram message to {}: {}", chat_id, msg.id.0);
                return Some(msg.id.0);
            }
            Err(e) => {
                error!(
                    "Attempt {} failed to send Telegram message to {}: {}",
                    attempt, chat_id, e
                );
                if attempt < self.max_retries {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        self.retry_delay_seconds,
                    ))
                    .await;
                }
            }
        }
    }

    error!(
        "Failed to send Telegram message to {} after {} attempts",
        chat_id, self.max_retries
    );
    None
}
```

2. Обновить `send_stream_notification()`:

```rust
pub async fn send_stream_notification(
    &self,
    chat_id: i64,
    streamer_login: &str,
    custom_message: &str,
    inline_buttons: Option<Vec<(String, String)>>,
) -> Option<i32> {
    let message = custom_message.to_string();

    self.send_with_retry(chat_id, || {
        let mut request = self
            .bot
            .send_message(ChatId(chat_id), &message)
            .parse_mode(teloxide::types::ParseMode::Html);

        if let Some(buttons) = &inline_buttons {
            let keyboard_results: Result<Vec<_>, _> = buttons
                .clone()
                .into_iter()
                .map(|(text, url)| {
                    Url::parse(&url)
                        .map(|parsed_url| vec![InlineKeyboardButton::url(text, parsed_url)])
                        .inspect_err(|&e| {
                            error!("Failed to parse URL '{}': {}", url, e);
                        })
                })
                .collect();

            match keyboard_results {
                Ok(keyboard) => {
                    let keyboard = InlineKeyboardMarkup::new(keyboard);
                    request = request.reply_markup(keyboard);
                }
                Err(_) => {
                    error!(
                        "Failed to create inline keyboard for streamer {}: invalid URL(s)",
                        streamer_login
                    );
                    // Continue without buttons
                }
            }
        }

        request
    })
    .await
}
```

3. Обновить `send_message()`:

```rust
pub async fn send_message(&self, chat_id: i64, text: &str) -> bool {
    self.send_with_retry(chat_id, || {
        self.bot
            .send_message(ChatId(chat_id), text)
            .parse_mode(teloxide::types::ParseMode::Html)
    })
    .await
    .is_some()
}
```

## 2. Функции длиннее 30 строк

**Проблема**: Несколько функций превышают 30 строк:

- `src/telegram/command_handler.rs:31-71` - `validate_streamer_login()` (41 строк)
- `src/telegram/command_handler.rs:326-385` - `parse_and_validate_channel()` (60 строк)
- `src/telegram/command_handler.rs:462-484` - `add_button_to_json()` (23 строки, но близко к лимиту)
- `src/state/manager.rs:119-177` - `handle_offline_to_online()` (59 строк)
- `src/state/manager.rs:291-339` - `handle_online_to_offline()` (49 строк)

**Решение**: Разбить на более мелкие функции

### Шаги:

1. Разбить `validate_streamer_login()` в `src/telegram/command_handler.rs`:

```rust
fn validate_streamer_login(&self, login: &str) -> Result<(), CommandError> {
    let trimmed = login.trim();

    self.validate_not_empty(trimmed, "username стримера")?;
    self.validate_length(trimmed, 25, "username стримера")?;
    self.validate_no_spaces(trimmed, "username стримера")?;
    self.validate_no_at_symbol(trimmed)?;
    self.validate_alphanumeric_underscore(trimmed)?;

    Ok(())
}

fn validate_not_empty(&self, text: &str, field_name: &str) -> Result<(), CommandError> {
    if text.is_empty() {
        return Err(CommandError::InvalidFormat(
            format!("❌ Укажите {}", field_name)
        ));
    }
    Ok(())
}

fn validate_length(&self, text: &str, max_len: usize, field_name: &str) -> Result<(), CommandError> {
    if text.len() > max_len {
        return Err(CommandError::InvalidFormat(
            format!("❌ {} слишком длинный (максимум {} символов)", field_name, max_len)
        ));
    }
    Ok(())
}

fn validate_no_spaces(&self, text: &str, field_name: &str) -> Result<(), CommandError> {
    if text.contains(' ') {
        return Err(CommandError::InvalidFormat(
            format!("❌ {} не должен содержать пробелов", field_name)
        ));
    }
    Ok(())
}

fn validate_no_at_symbol(&self, text: &str) -> Result<(), CommandError> {
    if text.contains('@') {
        return Err(CommandError::InvalidFormat(
            "❌ Укажите username без @".to_string()
        ));
    }
    Ok(())
}

fn validate_alphanumeric_underscore(&self, text: &str) -> Result<(), CommandError> {
    if !text.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(CommandError::InvalidFormat(
            "❌ Username стримера может содержать только строчные буквы a-z, цифры 0-9 и подчеркивания _".to_string()
        ));
    }
    Ok(())
}
```

2. Разбить `parse_and_validate_channel()` в `src/telegram/command_handler.rs`:

```rust
fn parse_and_validate_channel(&self, channel: &str) -> Result<String, CommandError> {
    self.validate_not_empty(channel, "чат для уведомлений")?;
    self.validate_length(channel, self.config.max_message_length, "идентификатор чата")?;

    if let Some(username) = channel.strip_prefix('@') {
        self.validate_username_channel(username)?;
        Ok(channel.to_string())
    } else {
        self.validate_numeric_channel(channel)
    }
}

fn validate_username_channel(&self, username: &str) -> Result<(), CommandError> {
    if username.is_empty() {
        return Err(CommandError::InvalidFormat(
            "❌ Укажите username после @. Формат: /set_channel @username".to_string()
        ));
    }
    if username.contains(' ') {
        return Err(CommandError::InvalidFormat(
            "❌ Username не должен содержать пробелов".to_string()
        ));
    }
    if username.len() > self.config.max_message_length {
        return Err(CommandError::InvalidFormat(
            format!("❌ Username чата слишком длинный (максимум {} символов)", self.config.max_message_length)
        ));
    }
    Ok(())
}

fn validate_numeric_channel(&self, channel: &str) -> Result<String, CommandError> {
    match channel.parse::<i64>() {
        Ok(id) => {
            if id == 0 {
                return Err(CommandError::InvalidFormat(
                    "❌ ID чата не может быть 0".to_string()
                ));
            }
            Ok(id.to_string())
        }
        Err(_) => {
            return Err(CommandError::InvalidFormat(
                "❌ Неверный формат ID чата. Укажите числовой ID или username с @".to_string()
            ));
        }
    }
}
```

3. Разбить `handle_offline_to_online()` в `src/state/manager.rs`:

```rust
async fn handle_offline_to_online(
    &self,
    state: &mut StreamState,
    streamer_login: &str,
    started_at: &str,
) {
    self.set_online_pending_state(state, streamer_login, started_at);
    self.spawn_grace_period_timer(state, streamer_login, "online").await;
}

fn set_online_pending_state(&self, state: &mut StreamState, streamer_login: &str, started_at: &str) {
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
}

async fn spawn_grace_period_timer(&self, state: &StreamState, streamer_login: &str, period_type: &str) {
    let state_manager = self.clone();
    let streamer_id_clone = state.streamer_id.clone();
    let streamer_login_clone = streamer_login.to_string();
    let grace_period_start = state.grace_period_start;
    let grace_period = if period_type == "online" {
        self.grace_period_online
    } else {
        self.grace_period_offline
    };

    tokio::spawn(async move {
        sleep(Duration::from_secs(grace_period)).await;

        let mut states = state_manager.states.write().await;
        if let Some(state) = states.get_mut(&streamer_id_clone) {
            if state.grace_period_start != grace_period_start {
                info!(
                    "Grace period for {} is no longer valid, ignoring timer",
                    streamer_login_clone
                );
                return;
            }

            if period_type == "online" {
                if state.status == StreamStatus::OnlinePending {
                    state_manager.confirm_online(state, &streamer_login_clone).await;
                } else if state.status == StreamStatus::Online {
                    info!(
                        "Stream {} already online, ignoring grace period completion",
                        streamer_login_clone
                    );
                } else {
                    info!(
                        "Stream {} went offline during grace period, cancelling notification",
                        streamer_login_clone
                    );
                }
            } else {
                if state.status == StreamStatus::OfflinePending {
                    state_manager.confirm_offline(state, &streamer_login_clone).await;
                } else if state.status == StreamStatus::Offline {
                    info!(
                        "Stream {} already offline, ignoring grace period completion",
                        streamer_login_clone
                    );
                } else {
                    info!(
                        "Stream {} went back online during grace period, cancelling deletion",
                        streamer_login_clone
                    );
                }
            }
        }
    });
}
```

4. Разбить `handle_online_to_offline()` в `src/state/manager.rs`:

```rust
async fn handle_online_to_offline(&self, state: &mut StreamState, streamer_login: &str) {
    self.set_offline_pending_state(state, streamer_login);
    self.spawn_grace_period_timer(state, streamer_login, "offline").await;
}

fn set_offline_pending_state(&self, state: &mut StreamState, streamer_login: &str) {
    state.status = StreamStatus::OfflinePending;
    state.grace_period_start = Some(Utc::now());

    info!(
        "Stream {} entered OfflinePending state, grace period: {}s",
        streamer_login, self.grace_period_offline
    );
}
```

## 3. Вынесение числовых констант в конфиг

**Проблема**: В коде есть "магические числа":

- `src/state/models.rs:53` - `30` секунд для дубликатов
- `src/twitch/poller.rs:57` - `100` (размер чанка)
- `src/telegram/command_handler.rs:25` - `25` (максимальная длина username)

**Решение**: Вынести в конфиг

### Шаги:

1. Добавить поля в `src/config.rs`:

```rust
pub struct Config {
    // ... существующие поля ...
    pub duplicate_event_window_seconds: i64,  // Для дубликатов
    pub twitch_batch_size: usize,             // Размер чанка для Twitch API
    pub max_username_length: usize,           // Максимальная длина username
}
```

2. Добавить значения по умолчанию в `Config::from_env()`:

```rust
let duplicate_event_window_seconds = env::var("DUPLICATE_EVENT_WINDOW_SECONDS")
    .unwrap_or_else(|_| "30".to_string())
    .parse::<i64>()
    .map_err(|e| {
        ConfigError::InvalidValue("DUPLICATE_EVENT_WINDOW_SECONDS".to_string(), e.to_string())
    })?;

let twitch_batch_size = env::var("TWITCH_BATCH_SIZE")
    .unwrap_or_else(|_| "100".to_string())
    .parse::<usize>()
    .map_err(|e| {
        ConfigError::InvalidValue("TWITCH_BATCH_SIZE".to_string(), e.to_string())
    })?;

let max_username_length = env::var("MAX_USERNAME_LENGTH")
    .unwrap_or_else(|_| "25".to_string())
    .parse::<usize>()
    .map_err(|e| {
        ConfigError::InvalidValue("MAX_USERNAME_LENGTH".to_string(), e.to_string())
    })?;
```

3. Использовать константы в коде:

```rust
// src/state/models.rs
pub fn is_duplicate_event(&self, event_id: &str, event_timestamp: &str) -> bool {
    // ... существующий код ...
    if (event_ts_utc - last_ts).num_seconds().abs() < self.duplicate_event_window_seconds {
        return true;
    }
    // ...
}

// src/twitch/poller.rs
for user_ids_chunk in user_ids.chunks(self.config.twitch_batch_size) {
    // ... существующий код ...
}

// src/telegram/command_handler.rs
self.validate_length(trimmed, self.config.max_username_length, "username стримера")?;
```

4. Добавить в `.env.example`:

```env
DUPLICATE_EVENT_WINDOW_SECONDS=30
TWITCH_BATCH_SIZE=100
MAX_USERNAME_LENGTH=25
```

## 4. Упрощение `handle_stream_online()` и `handle_stream_offline()`

**Проблема**: Эти функции слишком длинные и сложные, содержат много вложенных match.

**Решение**: Вынести логику обработки состояний в отдельные методы

### Шаги:

1. Создать методы обработки каждого состояния:

```rust
// src/state/manager.rs
impl StateManager {
    pub async fn handle_stream_online(
        &self,
        streamer_id: &str,
        streamer_login: &str,
        streamer_name: &str,
        event_id: &str,
        event_timestamp: &str,
        started_at: &str,
        target_chat_id: &str,
    ) {
        self.ensure_state(streamer_id, streamer_login, streamer_name).await;

        let mut states = self.states.write().await;
        let state = states.get_mut(streamer_id).unwrap();

        if state.is_duplicate_event(event_id, event_timestamp) {
            info!("Duplicate stream.online event detected for {}", streamer_login);
            return;
        }

        state.update_event_info(event_id.to_string(), event_timestamp);

        match state.status {
            StreamStatus::Offline => {
                self.handle_offline_to_online(state, streamer_login, started_at, target_chat_id).await;
            }
            StreamStatus::OnlinePending => {
                self.handle_online_pending_duplicate(streamer_login).await;
            }
            StreamStatus::Online => {
                self.handle_online_duplicate(streamer_login).await;
            }
            StreamStatus::OfflinePending => {
                self.handle_offline_pending_to_online(state, streamer_login).await;
            }
        }
    }

    async fn handle_offline_to_online(
        &self,
        state: &mut StreamState,
        streamer_login: &str,
        started_at: &str,
        target_chat_id: &str,
    ) {
        if target_chat_id.is_empty() {
            error!("Target chat not configured for {}, skipping notification", streamer_login);
            state.status = StreamStatus::Online;
            state.started_at = Some(/* ... */);
            return;
        }

        self.set_online_pending_state(state, streamer_login, started_at);
        self.spawn_grace_period_timer(state, streamer_login, "online").await;
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

    async fn handle_online_pending_duplicate(&self, streamer_login: &str) {
        info!(
            "Stream {} already in OnlinePending state, ignoring",
            streamer_login
        );
    }

    async fn handle_online_duplicate(&self, streamer_login: &str) {
        info!("Stream {} already online, ignoring", streamer_login);
    }
}
```

2. Аналогично для `handle_stream_offline()`:

```rust
pub async fn handle_stream_offline(
    &self,
    streamer_id: &str,
    streamer_login: &str,
    streamer_name: &str,
    event_id: &str,
    event_timestamp: &str,
) {
    self.ensure_state(streamer_id, streamer_login, streamer_name).await;

    let mut states = self.states.write().await;
    let state = states.get_mut(streamer_id).unwrap();

    if state.is_duplicate_event(event_id, event_timestamp) {
        info!("Duplicate stream.offline event detected for {}", streamer_login);
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
            self.handle_online_pending_to_offline(state, streamer_login).await;
        }
    }
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

async fn handle_offline_pending_duplicate(&self, streamer_login: &str) {
    info!(
        "Stream {} already in OfflinePending state, ignoring",
        streamer_login
    );
}

async fn handle_offline_duplicate(&self, streamer_login: &str) {
    info!("Stream {} already offline, ignoring", streamer_login);
}
```

## 5. Упрощение `handle_mysettings()`

**Проблема**: Функция содержит много логики форматирования ответа.

**Решение**: Вынести форматирование в отдельные методы

### Шаги:

1. Разбить `handle_mysettings()` в `src/telegram/command_handler.rs`:

```rust
async fn handle_mysettings(&self, user_id: i64) -> Result<String, CommandError> {
    let subscriptions_with_settings = self
        .storage
        .get_all_notification_settings_for_user(user_id)
        .await
        .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

    if subscriptions_with_settings.is_empty() {
        return Ok("❌ Настройки не установлены. Используйте /add username".to_string());
    }

    let mut response = self.format_settings_header();

    for (index, (subscription_id, twitch_user_id, settings)) in
        subscriptions_with_settings.iter().enumerate()
    {
        response.push_str(&self.format_subscription_info(
            index,
            subscription_id,
            twitch_user_id,
            settings,
        ));
        response.push('\n');
    }

    response.push_str(&self.format_settings_footer());

    Ok(response)
}

fn format_settings_header(&self) -> String {
    "📋 <b>Ваши подписки и настройки:</b>\n\n".to_string()
}

fn format_settings_footer(&self) -> String {
    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n\
    💡 <i>Используйте команды для настройки:</i>\n\
    • /set_channel - изменить чат\n\
    • /set_text - изменить текст\n\
    • /add_button - добавить кнопку\n\
    • /clear_buttons - удалить все кнопки".to_string()
}
```

## 6. Упрощение `handle_start()` и `handle_help()`

**Проблема**: Эти функции возвращают большие строки с хардкодом.

**Решение**: Вынести в конфиг или отдельный файл с шаблонами

### Шаги:

1. Создать файл `src/telegram/templates.rs`:

```rust
pub const START_MESSAGE: &str = r#"👋 <b>Привет!</b>

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
/add_button Перейти на стрим | https://twitch.tv/streamer123"#;

pub const HELP_MESSAGE: &str = r#"🤖 <b>Доступные команды:</b>

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
/help - Показать эту справку"#;
```

2. Обновить `src/telegram/mod.rs`:

```rust
pub mod bot;
pub mod command_handler;
pub mod commands;
pub mod templates;
```

3. Обновить `src/telegram/command_handler.rs`:

```rust
use crate::telegram::templates::{START_MESSAGE, HELP_MESSAGE};

fn handle_start(&self) -> String {
    START_MESSAGE.to_string()
}

pub fn handle_help(&self) -> String {
    HELP_MESSAGE.to_string()
}
```

## Итоговый порядок выполнения:

1. **Создать `src/telegram/templates.rs`** с сообщениями
2. **Обновить `src/telegram/mod.rs`** для экспорта templates
3. **Обновить `src/telegram/bot.rs`**:
   - Добавить `send_with_retry()`
   - Обновить `send_stream_notification()` и `send_message()`
4. **Обновить `src/telegram/command_handler.rs`**:
   - Разбить `validate_streamer_login()` на мелкие методы
   - Разбить `parse_and_validate_channel()` на мелкие методы
   - Разбить `handle_mysettings()` на мелкие методы
   - Обновить `handle_start()` и `handle_help()` для использования templates
5. **Обновить `src/state/manager.rs`**:
   - Разбить `handle_offline_to_online()` на мелкие методы
   - Разбить `handle_online_to_offline()` на мелкие методы
   - Создать `spawn_grace_period_timer()` для переиспользования
6. **Обновить `src/config.rs`**:
   - Добавить `duplicate_event_window_seconds`
   - Добавить `twitch_batch_size`
   - Добавить `max_username_length`
7. **Обновить `src/state/models.rs`**:
   - Использовать `duplicate_event_window_seconds` вместо `30`
8. **Обновить `src/twitch/poller.rs`**:
   - Использовать `twitch_batch_size` вместо `100`
9. **Обновить `src/telegram/command_handler.rs`**:
   - Использовать `max_username_length` вместо `25`
10. **Обновить `.env.example`** с новыми константами
