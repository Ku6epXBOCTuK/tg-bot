# План исправления важных проблем

## 1. Неполная валидация ввода пользователя

**Проблема**: `validate_streamer_login()` разрешает дефисы, но Twitch не разрешает их в username. Нет проверки на максимальную длину username Twitch (25 символов).

**Решение**: Обновить валидацию username

### Шаги:

1. Изменить `validate_streamer_login()` в `src/telegram/command_handler.rs`:

```rust
fn validate_streamer_login(&self, login: &str) -> Result<(), CommandError> {
    let trimmed = login.trim();

    if trimmed.is_empty() {
        return Err(CommandError::InvalidFormat(
            "❌ Укажите username стримера".to_string(),
        ));
    }

    // Проверка максимальной длины Twitch username (25 символов)
    if trimmed.len() > 25 {
        return Err(CommandError::InvalidFormat(
            "❌ Username стримера слишком длинный (максимум 25 символов)".to_string(),
        ));
    }

    if trimmed.len() > self.config.max_message_length {
        return Err(CommandError::InvalidFormat(format!(
            "❌ Username стримера слишком длинный (максимум {} символов)",
            self.config.max_message_length
        )));
    }

    if trimmed.contains(' ') {
        return Err(CommandError::InvalidFormat(
            "❌ Username стримера не должен содержать пробелов".to_string(),
        ));
    }

    if trimmed.contains('@') {
        return Err(CommandError::InvalidFormat(
            "❌ Укажите username без @".to_string(),
        ));
    }

    // Проверка для valid characters (только a-z, 0-9, _)
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(CommandError::InvalidFormat(
            "❌ Username стримера может содержать только строчные буквы a-z, цифры 0-9 и подчеркивания _"
                .to_string(),
        ));
    }

    Ok(())
}
```

## 2. Проблема с rate limiting Twitch API

**Проблема**: При получении 429 бот ждет `Retry-After` секунд, но не учитывает, что лимит 120 запросов/мин. Если запросить 100 streamers за раз, это исчерпает лимит на 1 минуту.

**Решение**: Добавить batching и глобальный rate limiter

### Шаги:

1. Добавить глобальный rate limiter в `src/twitch/api.rs`:

```rust
use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct TwitchApiClient {
    client: Client,
    config: Config,
    access_token: Option<String>,
    token_expires_at: Option<std::time::Instant>,
    rate_limiter: Arc<Mutex<RateLimiter<u32, Quota>>>,  // Глобальный rate limiter
}

impl TwitchApiClient {
    pub fn new(config: Config) -> Self {
        // 120 запросов в минуту
        let quota = Quota::per_minute(nonzero!(120u32));
        let rate_limiter = RateLimiter::direct(quota);

        Self {
            client: Client::new(),
            config,
            access_token: None,
            token_expires_at: None,
            rate_limiter: Arc::new(Mutex::new(rate_limiter)),
        }
    }

    async fn wait_for_rate_limit(&self) {
        let limiter = self.rate_limiter.lock().await;
        limiter.until_ready().await;
    }
}
```

2. Добавить batching в `src/twitch/poller.rs`:

```rust
pub async fn start_polling(&mut self) {
    // ... существующий код ...

    // Разбиваем на чанки по 100 user_id
    for user_ids_chunk in user_ids.chunks(100) {
        // Ждем rate limiter
        self.client.wait_for_rate_limit().await;

        match self.client.get_streams(user_ids_chunk).await {
            Ok(streams) => {
                // ... обработка ...
            }
            Err(e) => {
                error!("Failed to get streams: {}", e);
            }
        }
    }
}
```

3. Обновить `Cargo.toml`:

```toml
[dependencies]
governor = "0.6"
nonzero_ext = "0.3"
```

## 3. Отсутствие обработки ошибок в `handle_stream_online()`

**Проблема**: Если `get_user_id()` вернет ошибку, бот продолжит работу без уведомления. Нет логирования ошибок для администратора.

**Решение**: Добавить обработку ошибок и уведомление администратора

### Шаги:

1. Добавить поле `admin_id` в `src/config.rs`:

```rust
pub struct Config {
    // ... существующие поля ...
    pub admin_id: Option<i64>,  // Telegram ID администратора
}
```

2. Добавить метод уведомления администратора в `src/telegram/bot.rs`:

```rust
pub async fn notify_admin(&self, admin_id: i64, message: &str) -> bool {
    self.send_message(admin_id, message).await
}
```

3. Обновить `handle_add()` и `handle_remove()` в `src/telegram/command_handler.rs`:

```rust
async fn handle_add(&self, user_id: i64, streamer_login: &str) -> Result<String, CommandError> {
    // ... валидация ...

    let mut client = self.twitch_client.clone();
    let twitch_user_id = match client.get_user_id(streamer_login).await {
        Ok(id) => id,
        Err(e) => {
            // Логируем ошибку для администратора
            if let Some(admin_id) = self.config.admin_id {
                let error_msg = format!(
                    "Ошибка при получении user_id для {}: {}",
                    streamer_login, e
                );
                self.bot.notify_admin(admin_id, &error_msg).await;
            }
            return Err(CommandError::TwitchApiError(e.to_string()));
        }
    };

    // ... остальной код ...
}
```

## 4. Дублирование кода отправки сообщений

**Проблема**: `send_stream_notification()` и `send_message()` имеют одинаковую логику retry.

**Решение**: Вынести общую логику в отдельную функцию

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
            // ... создание клавиатуры ...
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

## 5. Отсутствие prepared statements в `get_streamer_configs()`

**Проблема**: `get_streamer_configs()` использует `sqlx::query()` без `bind()`, что не является prepared statement.

**Решение**: Использовать `sqlx::query_as()` с привязкой параметров

### Шаги:

1. Создать структуру для результата:

```rust
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
```

2. Использовать `query_as()`:

```rust
pub async fn get_streamer_configs(&self) -> Result<Vec<StreamerConfig>, StorageError> {
    let configs = sqlx::query_as!(
        StreamerConfig,
        r#"
        SELECT
            streamer_id,
            streamer_login,
            streamer_name,
            created_at,
            eventsub_subscription_id,
            online_subscription_id,
            offline_subscription_id
        FROM streamers
        "#,
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(configs)
}
```

## 6. Отсутствие валидации URL в `validate_url()`

**Проблема**: `validate_url()` проверяет только протокол, но не валидирует сам URL.

**Решение**: Использовать `url::Url` для валидации

### Шаги:

1. Обновить `validate_url()` в `src/telegram/command_handler.rs`:

```rust
fn validate_url(&self, url: &str) -> Result<(), CommandError> {
    let trimmed = url.trim();

    if trimmed.is_empty() {
        return Err(CommandError::InvalidFormat(
            "❌ URL не может быть пустым".to_string(),
        ));
    }

    if trimmed.len() > self.config.max_message_length {
        return Err(CommandError::InvalidFormat(format!(
            "❌ URL слишком длинный (максимум {} символов)",
            self.config.max_message_length
        )));
    }

    // Полная валидация URL
    match url::Url::parse(trimmed) {
        Ok(parsed_url) => {
            // Проверяем, что URL имеет http или https протокол
            if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                return Err(CommandError::InvalidFormat(
                    "❌ URL должен использовать http:// или https://".to_string(),
                ));
            }
        }
        Err(_) => {
            return Err(CommandError::InvalidFormat(
                "❌ Неверный формат URL".to_string(),
            ));
        }
    }

    Ok(())
}
```

2. Обновить `Cargo.toml`:

```toml
[dependencies]
url = "2.4"
```

## 7. Отсутствие ограничения на количество подписок

**Проблема**: Пользователь может добавить неограниченное количество стримеров, что может привести к:

- Превышению лимитов Twitch API
- Медленной работе бота
- Высокому потреблению памяти

**Решение**: Добавить ограничение в конфиг

### Шаги:

1. Добавить поле в `src/config.rs`:

```rust
pub struct Config {
    // ... существующие поля ...
    pub max_subscriptions_per_user: usize,
}
```

2. Добавить валидацию в `handle_add()`:

```rust
async fn handle_add(&self, user_id: i64, streamer_login: &str) -> Result<String, CommandError> {
    // ... валидация ...

    // Проверка лимита подписок
    let subscriptions = self.storage.get_user_subscriptions(user_id).await
        .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

    if subscriptions.len() >= self.config.max_subscriptions_per_user {
        return Err(CommandError::InvalidFormat(format!(
            "❌ Достигнут лимит подписок (максимум {})",
            self.config.max_subscriptions_per_user
        )));
    }

    // ... остальной код ...
}
```

3. Добавить в `.env.example`:

```env
MAX_SUBSCRIPTIONS_PER_USER=10
```

## Итоговый порядок выполнения:

1. **Обновить `src/telegram/command_handler.rs`**:
   - Обновить `validate_streamer_login()`
   - Обновить `validate_url()`
   - Добавить проверку лимита подписок в `handle_add()`
   - Добавить обработку ошибок в `handle_add()` и `handle_remove()`

2. **Обновить `src/twitch/api.rs`**:
   - Добавить глобальный rate limiter
   - Добавить метод `wait_for_rate_limit()`

3. **Обновить `src/twitch/poller.rs`**:
   - Добавить batching (чанки по 100 user_id)
   - Добавить вызов `wait_for_rate_limit()`

4. **Обновить `src/telegram/bot.rs`**:
   - Добавить приватный метод `send_with_retry()`
   - Обновить `send_stream_notification()` и `send_message()`
   - Добавить метод `notify_admin()`

5. **Обновить `src/config.rs`**:
   - Добавить `admin_id`
   - Добавить `max_subscriptions_per_user`

6. **Обновить `src/storage/sqlite.rs`**:
   - Использовать `query_as()` в `get_streamer_configs()`

7. **Обновить `Cargo.toml`**:
   - Добавить `governor = "0.6"`
   - Добавить `nonzero_ext = "0.3"`
   - Добавить `url = "2.4"`

8. **Обновить `.env.example`**:
   - Добавить `ADMIN_ID`
   - Добавить `MAX_SUBSCRIPTIONS_PER_USER`
