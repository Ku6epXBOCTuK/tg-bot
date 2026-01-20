# План исправления критических проблем

## 1. Падение бота при отправке уведомлений в несуществующий чат

**Проблема**: В `src/state/manager.rs:188-195` отправляется уведомление с `chat_id = 0`, что приведет к ошибке. Нет валидации, что `target_chat_id` настроен перед отправкой.

**Решение**: Добавить проверку перед вызовом `confirm_online()`

### Шаги:

1. Изменить `handle_stream_online()` в `src/state/manager.rs`:
   - Добавить параметр `target_chat_id` в метод
   - Проверять, что `target_chat_id` не пустой перед вызовом `confirm_online()`
   - Если пустой, логировать ошибку и переходить в статус `Online` без уведомления

2. Изменить `TwitchPoller::start_polling()` в `src/twitch/poller.rs`:
   - Получать `target_chat_id` для каждого стримера из БД
   - Передавать его в `handle_stream_online()`

### Код:

```rust
// src/state/manager.rs
pub async fn handle_stream_online(
    &self,
    streamer_id: &str,
    streamer_login: &str,
    streamer_name: &str,
    event_id: &str,
    event_timestamp: &str,
    started_at: &str,
    target_chat_id: &str,  // Новый параметр
) {
    // ... существующий код ...

    match state.status {
        StreamStatus::Offline => {
            // Проверка перед вызовом
            if target_chat_id.is_empty() {
                error!("Target chat not configured for {}, skipping notification", streamer_login);
                state.status = StreamStatus::Online;
                state.started_at = Some(/* ... */);
                return;
            }
            self.handle_offline_to_online(state, streamer_login, started_at, target_chat_id)
                .await;
        }
        // ... остальной код ...
    }
}

// src/twitch/poller.rs
pub async fn start_polling(&mut self) {
    // ... существующий код ...

    for user_id in &user_ids {
        let is_online = streams.iter().any(|s| &s.user_id == user_id);

        // Получаем target_chat_id для этого стримера
        let target_chat_id = self.storage.get_target_chat_for_user(user_id).await
            .unwrap_or_default();

        if is_online {
            let stream = streams.iter().find(|s| &s.user_id == user_id).unwrap();
            self.state_manager
                .handle_stream_online(
                    user_id,
                    &stream.user_login,
                    &stream.user_name,
                    &format!("{}_{}", user_id, stream.started_at),
                    &stream.started_at,
                    &stream.started_at,
                    &target_chat_id,  // Передаем target_chat_id
                )
                .await;
        } else {
            // ... offline handling ...
        }
    }
}
```

3. Добавить метод в `src/storage/sqlite.rs`:

```rust
pub async fn get_target_chat_for_user(&self, twitch_user_id: &str) -> Result<String, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT ns.target_chat_id
        FROM user_subscriptions us
        LEFT JOIN notification_settings ns ON us.id = ns.subscription_id
        WHERE us.twitch_user_id = ?
        LIMIT 1
        "#,
    )
    .bind(twitch_user_id)
    .fetch_optional(&self.pool)
    .await?;

    Ok(row.map(|r| r.get("target_chat_id")).unwrap_or_default())
}
```

## 2. Race condition при быстром перезапуске стрима

**Проблема**: При перезапуске стрима во время grace period создается новый таймер, но старый не отменяется. Старый таймер может отправить уведомление после того, как стрим уже перезапущен.

**Решение**: Использовать `last_change_time` в БД вместо фоновых задач

### Шаги:

1. Создать таблицу для хранения состояния стримов:

```sql
-- migrations/002_stream_state.sql
CREATE TABLE stream_state (
    twitch_user_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    started_at TEXT,
    pending_started_at TEXT,
    telegram_message_id INTEGER,
    last_event_id TEXT,
    last_event_timestamp TEXT,
    grace_period_start TEXT,
    updated_at TEXT NOT NULL
);
```

2. Изменить `StateManager` в `src/state/manager.rs`:
   - Убрать `Arc<RwLock<HashMap<String, StreamState>>>`
   - Использовать `Storage` для всех операций
   - Убрать фоновые задачи (tokio::spawn)

3. Переписать `handle_stream_online()` и `handle_stream_offline()`:
   - Сохранять состояние в БД
   - Проверять `grace_period_start` из БД перед отправкой уведомления
   - Использовать `updated_at` для обнаружения race conditions

### Код:

```rust
// src/state/manager.rs
pub struct StateManager {
    storage: Storage,  // Вместо HashMap
    telegram_bot: Arc<Mutex<TelegramBot>>,
    grace_period_online: u64,
    grace_period_offline: u64,
}

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
        // Сохраняем состояние в БД
        self.storage.save_stream_state(
            streamer_id,
            "OnlinePending",
            started_at,
            event_id,
            event_timestamp,
            &Utc::now().to_rfc3339(),
        ).await;

        // Запускаем таймер grace period
        let state_manager = self.clone();
        let streamer_id_clone = streamer_id.to_string();
        let streamer_login_clone = streamer_login.to_string();
        let grace_period_start = Utc::now();

        tokio::spawn(async move {
            sleep(Duration::from_secs(state_manager.grace_period_online)).await;

            // Проверяем, что состояние не изменилось
            let current_state = state_manager.storage.get_stream_state(&streamer_id_clone).await;
            if let Ok(Some(state)) = current_state {
                if state.grace_period_start == grace_period_start.to_rfc3339() &&
                   state.status == "OnlinePending" {
                    // Отправляем уведомление
                    state_manager.confirm_online(&streamer_id_clone, &streamer_login_clone, target_chat_id).await;
                }
            }
        });
    }
}
```

## 3. N+1 проблема в `get_all_twitch_user_ids()`

**Проблема**: Запрос возвращает `DISTINCT twitch_user_id`, но при этом каждый стример может иметь несколько подписок. При большом количестве пользователей это создаст дубликаты в списке.

**Решение**: Использовать `GROUP BY twitch_user_id`

### Шаги:

1. Изменить метод в `src/storage/sqlite.rs`:

```rust
pub async fn get_all_twitch_user_ids(&self) -> Result<Vec<String>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT twitch_user_id
        FROM user_subscriptions
        GROUP BY twitch_user_id
        "#,
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get("twitch_user_id"))
        .collect())
}
```

## 4. Отсутствие проверки прав доступа

**Проблема**: Любой пользователь может изменить настройки любого другого пользователя. Команды `/set_channel`, `/set_text`, `/add_button` работают с последней подпиской без проверки владельца.

**Решение**: Добавить проверку `user_id` в `get_last_subscription_with_settings()`

### Шаги:

1. Изменить метод в `src/telegram/command_handler.rs`:

```rust
async fn get_last_subscription_with_settings(
    &self,
    user_id: i64,
) -> Result<(i64, String, Option<NotificationSettings>), CommandError> {
    let subscriptions = self
        .storage
        .get_user_subscriptions(user_id)
        .await
        .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

    let last_sub = subscriptions.last().ok_or_else(|| {
        CommandError::InvalidFormat("❌ Сначала добавьте стримера с помощью /add".to_string())
    })?;

    // Проверка, что подписка принадлежит user_id
    if last_sub.user_telegram_id != user_id {
        return Err(CommandError::InvalidFormat(
            "❌ Нет доступа к этой подписке".to_string(),
        ));
    }

    let settings = self
        .storage
        .get_notification_settings(last_sub.id)
        .await
        .map_err(|e| CommandError::DatabaseError(e.to_string()))?;

    Ok((last_sub.id, last_sub.twitch_user_id.clone(), settings))
}
```

2. Изменить `UserSubscription` в `src/storage/sqlite.rs`:

```rust
#[derive(Clone, Debug)]
pub struct UserSubscription {
    pub id: i64,
    pub user_telegram_id: i64,  // Добавить поле
    pub twitch_user_id: String,
}
```

3. Изменить `get_user_subscriptions()`:

```rust
pub async fn get_user_subscriptions(
    &self,
    user_telegram_id: i64,
) -> Result<Vec<UserSubscription>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_telegram_id, twitch_user_id
        FROM user_subscriptions
        WHERE user_telegram_id = ?
        "#,
    )
    .bind(user_telegram_id)
    .fetch_all(&self.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| UserSubscription {
            id: row.get("id"),
            user_telegram_id: row.get("user_telegram_id"),
            twitch_user_id: row.get("twitch_user_id"),
        })
        .collect())
}
```

## 5. Потеря данных при быстром перезапуске

**Проблема**: Состояние хранится в памяти (`HashMap`), при перезапуске бота все данные теряются. Нет восстановления состояния из БД при старте.

**Решение**: Сохранять состояние в БД или восстанавливать при старте

### Шаги:

1. Создать таблицу `stream_state` (см. пункт 2)
2. Добавить методы восстановления в `src/state/manager.rs`:

```rust
pub async fn restore_state(&self) {
    let states = self.storage.get_all_stream_states().await;
    for state in states {
        // Восстановить состояние в памяти
        self.states.write().await.insert(state.streamer_id.clone(), state);
    }
}
```

3. Вызвать `restore_state()` в `src/main.rs` после инициализации `StateManager`:

```rust
let state_manager = Arc::new(StateManager::new(...));
state_manager.restore_state().await;
```

## Итоговый порядок выполнения:

1. **Создать миграцию** `migrations/002_stream_state.sql`
2. **Обновить `src/storage/sqlite.rs`**:
   - Добавить `get_target_chat_for_user()`
   - Добавить `get_all_stream_states()`
   - Добавить `save_stream_state()`
   - Обновить `UserSubscription` и `get_user_subscriptions()`
   - Обновить `get_all_twitch_user_ids()`
3. **Обновить `src/state/models.rs`**:
   - Добавить методы для работы с БД
4. **Обновить `src/state/manager.rs`**:
   - Переписать на использование БД
   - Убрать HashMap
   - Добавить `restore_state()`
5. **Обновить `src/twitch/poller.rs`**:
   - Получать `target_chat_id` для каждого стримера
   - Передавать в `handle_stream_online()`
6. **Обновить `src/telegram/command_handler.rs`**:
   - Добавить проверку прав доступа
7. **Обновить `src/main.rs`**:
   - Вызвать `restore_state()` после инициализации
