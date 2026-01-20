# План исправления критических ошибок Telegram бота

## Критические проблемы (порядок исправления)

### 1. ✅ Race condition в grace period (src/state/manager.rs)

**Проблема:** При быстром перезапуске стрима фоновый таймер может отправить уведомление о неактуальном стриме.

**Решение:** Добавить проверку актуальности grace period в таймере.

**Статус:** Исправлено. Добавлена проверка `grace_period_start` для защиты от race condition.

### 2. ✅ Падение при некорректном JSON кнопок (src/telegram/command_handler.rs:310)

**Проблема:** `unwrap()` на сериализации JSON вызовет панику.

**Решение:** Заменить на `?` с обработкой ошибки.

**Статус:** Исправлено. Использован `map_err` с возвратом `CommandError::InvalidFormat`.

### 3. ✅ Падение при отсутствии подписок (src/telegram/command_handler.rs)

**Проблема:** `unwrap()` на `subscriptions.last()` вызовет панику.

**Решение:** Заменить на `ok_or_else()`.

**Статус:** Исправлено. Все 6 вызовов `subscriptions.last().unwrap()` заменены на `ok_or_else()`.

### 4. ✅ Падение при некорректном URL (src/telegram/bot.rs:70)

**Проблема:** `unwrap()` на парсинге URL вызовет панику.

**Решение:** Заменить на `?` с обработкой ошибки.

**Статус:** Исправлено. Использован `inspect_err` для логирования ошибок без паники.

### 5. ✅ Отсутствие обработки 429 от Twitch API (src/twitch/api.rs)

**Проблема:** Бот не делает паузу при rate limit.

**Решение:** Добавить обработку HTTP 429 с задержкой.

**Статус:** Исправлено. Добавлена обработка HTTP 429 с чтением `Retry-After` заголовка во всех API методах.

### 6. ✅ Отсутствие retry логики для Twitch API (src/twitch/api.rs)

**Проблема:** Сетевые ошибки не обрабатываются с повторными попытками.

**Решение:** Добавить retry с экспоненциальной задержкой.

**Статус:** Исправлено. Добавлен retry с экспоненциальной задержкой (base: 2s, max: 3 попытки) во всех API методах.

### 7. ✅ Отсутствие валидации ввода (src/telegram/command_handler.rs)

**Проблема:** Можно добавить некорректные данные (слишком длинные, пустые, опасные).

**Решение:** Добавить проверки длины, формата, безопасности.

**Статус:** Исправлено. Добавлена комплексная валидация:

- `validate_streamer_login()` - проверка username стримера (длина, пробелы, @, допустимые символы)
- `validate_text()` - проверка текста уведомления (длина, не пустой)
- `validate_button_text()` - проверка текста кнопки (длина, не пустой)
- `validate_url()` - проверка URL (длина, формат http/https)
- `validate_button_limit()` - проверка лимита кнопок (максимум 10)
- Константы: MAX_STREAMER_LOGIN_LENGTH=25, MAX_TEXT_LENGTH=1000, MAX_BUTTON_TEXT_LENGTH=64, MAX_URL_LENGTH=2048, MAX_BUTTONS_PER_SUBSCRIPTION=10

### 8. ✅ Отсутствие ограничений (src/telegram/command_handler.rs)

**Проблема:** Можно добавить неограниченное количество подписок/кнопок.

**Решение:** Добавить лимиты.

**Статус:** Исправлено. Добавлены ограничения:

- MAX_BUTTONS_PER_SUBSCRIPTION=10 - лимит кнопок на подписку
- Проверка лимита перед добавлением новой кнопки

### 9. ✅ N+1 запросы (src/telegram/command_handler.rs:77-146)

**Проблема:** Для каждой подписки отдельный запрос за настройками.

**Решение:** Загружать одним запросом.

**Статус:** Исправлено. Добавлен метод `get_all_notification_settings_for_user()` в `src/storage/sqlite.rs`, который выполняет JOIN запрос для получения всех подписок и настроек пользователя за один запрос. Функция `handle_mysettings()` теперь использует этот метод вместо цикла с отдельными запросами.

### 10. ✅ Отсутствие индексов (migrations/001_initial_schema.sql)

**Проблема:** Медленные запросы при большом количестве данных.

**Решение:** Добавить индексы.

**Статус:** Исправлено. Добавлен индекс `idx_notification_settings_subscription` на поле `subscription_id` в таблице `notification_settings`. Существующие индексы:

- `idx_streamers_login` - streamers(streamer_login)
- `idx_stream_states_status` - stream_states(status)
- `idx_user_subscriptions_user` - user_subscriptions(user_telegram_id)
- `idx_user_subscriptions_twitch` - user_subscriptions(twitch_user_id)
- `idx_notification_settings_subscription` - notification_settings(subscription_id)

## Дополнительные улучшения

### 11. ✅ Вынести константы в конфиг

**Статус:** Исправлено. Все константы вынесены в `src/config.rs`:

- `max_retries: 3` - максимальное количество попыток
- `retry_delay_seconds: 2` - задержка между попытками
- `duplicate_event_threshold_seconds: 30` - порог дублирования событий
- `token_refresh_buffer_seconds: 60` - буфер обновления токена
- `max_subscriptions_per_user: 10` - лимит подписок на пользователя
- `max_buttons_per_subscription: 10` - лимит кнопок на подписку
- `max_message_length: 1000` - максимальная длина сообщения

Константы используются в:

- `src/telegram/bot.rs` - max_retries, retry_delay_seconds
- `src/twitch/api.rs` - max_retries, retry_delay_seconds
- `src/telegram/command_handler.rs` - все валидационные константы

### 12. Разбить длинные функции

- handle_mysettings() → 3 функции
- handle_set_channel() → 2 функции
- handle_add_button() → 2 функции
- handle_stream_online() → 3 функции
- handle_stream_offline() → 3 функции
- Storage::new() → 4 функции

### 13. Устранить дублирование

- get_last_subscription()
- update_settings()
- send_message_with_retry()
- log_retry_error()

## Последовательность исправлений

1. **Сначала критические падения** (unwrap, panic)
2. **Затем race condition** (grace period)
3. **Потом надежность** (retry, 429)
4. **Затем валидация** (ввод, лимиты)
5. **Потом производительность** (N+1, индексы)
6. **В конце чистота кода** (рефакторинг, константы)
