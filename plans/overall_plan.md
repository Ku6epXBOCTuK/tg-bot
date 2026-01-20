# Итоговый план исправления Telegram-бота на Rust

## Приоритет 1: Критические проблемы (безопасность и стабильность)

### 1.1 Падение бота при отправке уведомлений в несуществующий чат

**Проблема**: В `src/state/manager.rs:188-195` отправляется уведомление с `chat_id = 0`, что приведет к ошибке. Нет валидации, что `target_chat_id` настроен перед отправкой.

**Решение**: Добавить проверку перед вызовом `confirm_online()`

**Действия**:

1. Добавить метод `get_target_chat_for_user()` в [`src/storage/sqlite.rs`](src/storage/sqlite.rs)
2. Изменить `handle_stream_online()` в [`src/state/manager.rs`](src/state/manager.rs):
   - Добавить параметр `target_chat_id`
   - Проверять, что `target_chat_id` не пустой перед отправкой
   - Если пустой, логировать ошибку и переходить в статус `Online` без уведомления
3. Изменить `start_polling()` в [`src/twitch/poller.rs`](src/twitch/poller.rs):
   - Получать `target_chat_id` для каждого стримера
   - Передавать в `handle_stream_online()`

**Риск**: Бот падает при попытке отправить уведомление в несуществующий чат

### 1.2 Race condition при быстром перезапуске стрима

**Проблема**: При перезапуске стрима во время grace period создается новый таймер, но старый не отменяется. Старый таймер может отправить уведомление после того, как стрим уже перезапущен.

**Решение**: Использовать `last_change_time` в БД вместо фоновых задач

**Действия**:

1. Создать миграцию `migrations/002_stream_state.sql` с таблицей `stream_state`
2. Переписать `StateManager` в [`src/state/manager.rs`](src/state/manager.rs):
   - Убрать `Arc<RwLock<HashMap<String, StreamState>>>`
   - Использовать `Storage` для всех операций
   - Убрать фоновые задачи (tokio::spawn)
3. Переписать `handle_stream_online()` и `handle_stream_offline()`:
   - Сохранять состояние в БД
   - Проверять `grace_period_start` из БД перед отправкой уведомления
   - Использовать `updated_at` для обнаружения race conditions

**Риск**: Старый таймер может отправить уведомление после перезапуска стрима

### 1.3 N+1 проблема в `get_all_twitch_user_ids()`

**Проблема**: Запрос возвращает `DISTINCT twitch_user_id`, но при этом каждый стример может иметь несколько подписок. При большом количестве пользователей это создаст дубликаты в списке.

**Решение**: Использовать `GROUP BY twitch_user_id`

**Действия**:

1. Изменить запрос в [`src/storage/sqlite.rs`](src/storage/sqlite.rs) с `DISTINCT twitch_user_id` на `GROUP BY twitch_user_id`

**Риск**: Дубликаты в списке стримеров при большом количестве пользователей

### 1.4 Отсутствие проверки прав доступа

**Проблема**: Любой пользователь может изменить настройки любого другого пользователя. Команды `/set_channel`, `/set_text`, `/add_button` работают с последней подпиской без проверки владельца.

**Решение**: Добавить проверку `user_id` в `get_last_subscription_with_settings()`

**Действия**:

1. Добавить поле `user_telegram_id` в `UserSubscription` в [`src/storage/sqlite.rs`](src/storage/sqlite.rs)
2. Обновить `get_user_subscriptions()` для возврата `user_telegram_id`
3. Добавить проверку в `get_last_subscription_with_settings()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - Проверять, что подписка принадлежит `user_id`
   - Возвращать ошибку, если нет доступа

**Риск**: Любой пользователь может изменить настройки другого пользователя

### 1.5 Потеря данных при быстром перезапуске

**Проблема**: Состояние хранится в памяти (`HashMap`), при перезапуске бота все данные теряются. Нет восстановления состояния из БД при старте.

**Решение**: Сохранять состояние в БД или восстанавливать при старте

**Действия**:

1. Создать таблицу `stream_state` (см. пункт 1.2)
2. Добавить методы восстановления в [`src/state/manager.rs`](src/state/manager.rs):
   - `restore_state()` - восстановление из БД
3. Вызвать `restore_state()` в [`src/main.rs`](src/main.rs) после инициализации `StateManager`

**Риск**: Все данные о состоянии стримов теряются при перезапуске бота

## Приоритет 2: Важные проблемы (производительность и безопасность)

### 2.1 Неполная валидация ввода пользователя

**Проблема**: `validate_streamer_login()` разрешает дефисы, но Twitch не разрешает их в username. Нет проверки на максимальную длину username Twitch (25 символов).

**Решение**: Обновить валидацию username

**Действия**:

1. Обновить `validate_streamer_login()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - Проверка максимальной длины 25 символов
   - Разрешить только `a-z`, `0-9`, `_`
   - Убрать дефисы из разрешенных символов

**Риск**: Некорректные username могут привести к ошибкам Twitch API

### 2.2 Проблема с rate limiting Twitch API

**Проблема**: При получении 429 бот ждет `Retry-After` секунд, но не учитывает, что лимит 120 запросов/мин. Если запросить 100 streamers за раз, это исчерпает лимит на 1 минуту.

**Решение**: Добавить batching и глобальный rate limiter

**Действия**:

1. Добавить глобальный rate limiter в [`src/twitch/api.rs`](src/twitch/api.rs):
   - Использовать `governor` crate
   - Лимит 120 запросов/минуту
2. Добавить batching в [`src/twitch/poller.rs`](src/twitch/poller.rs):
   - Разбивать user_ids на чанки по 100
   - Вызывать `wait_for_rate_limit()` перед каждым запросом
3. Обновить `Cargo.toml`:
   - Добавить `governor = "0.6"`
   - Добавить `nonzero_ext = "0.3"`

**Риск**: Превышение лимитов Twitch API, блокировка аккаунта

### 2.3 Отсутствие обработки ошибок в `handle_stream_online()`

**Проблема**: Если `get_user_id()` вернет ошибку, бот продолжит работу без уведомления. Нет логирования ошибок для администратора.

**Решение**: Добавить обработку ошибок и уведомление администратора

**Действия**:

1. Добавить поле `admin_id` в [`src/config.rs`](src/config.rs)
2. Добавить метод `notify_admin()` в [`src/telegram/bot.rs`](src/telegram/bot.rs)
3. Обновить `handle_add()` и `handle_remove()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - Логировать ошибки для администратора
   - Уведомлять администратора о критических ошибках

**Риск**: Ошибки остаются незамеченными, сложная отладка

### 2.4 Дублирование кода отправки сообщений

**Проблема**: `send_stream_notification()` и `send_message()` имеют одинаковую логику retry.

**Решение**: Вынести общую логику в отдельную функцию

**Действия**:

1. Создать приватный метод `send_with_retry()` в [`src/telegram/bot.rs`](src/telegram/bot.rs)
2. Обновить `send_stream_notification()` и `send_message()` для использования `send_with_retry()`

**Риск**: Поддержка кода усложняется, возможны ошибки при изменении логики

### 2.5 Отсутствие prepared statements в `get_streamer_configs()`

**Проблема**: `get_streamer_configs()` использует `sqlx::query()` без `bind()`, что не является prepared statement.

**Решение**: Использовать `sqlx::query_as()` с привязкой параметров

**Действия**:

1. Использовать `sqlx::query_as()` в [`src/storage/sqlite.rs`](src/storage/sqlite.rs)

**Риск**: Медленнее выполнение запросов, потенциальные SQL-инъекции (хотя в данном случае параметров нет)

### 2.6 Отсутствие валидации URL

**Проблема**: `validate_url()` проверяет только протокол, но не валидирует сам URL.

**Решение**: Использовать `url::Url` для валидации

**Действия**:

1. Обновить `validate_url()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - Использовать `url::Url` для полной валидации
   - Проверять протокол (http/https)
2. Обновить `Cargo.toml`:
   - Добавить `url = "2.4"`

**Риск**: Некорректные URL в кнопках

### 2.7 Отсутствие ограничения на количество подписок

**Проблема**: Пользователь может добавить неограниченное количество стримеров, что может привести к превышению лимитов Twitch API и высокому потреблению памяти.

**Решение**: Добавить ограничение в конфиг

**Действия**:

1. Добавить поле `max_subscriptions_per_user` в [`src/config.rs`](src/config.rs)
2. Добавить проверку в `handle_add()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs)
3. Добавить в `.env.example`

**Риск**: Превышение лимитов Twitch API, высокое потребление памяти

## Приоритет 3: Рефакторинг кода

### 3.1 Упрощение функций длиннее 30 строк

**Проблема**: Несколько функций превышают 30 строк:

- `src/telegram/command_handler.rs:31-71` - `validate_streamer_login()` (41 строк)
- `src/telegram/command_handler.rs:326-385` - `parse_and_validate_channel()` (60 строк)
- `src/state/manager.rs:119-177` - `handle_offline_to_online()` (59 строк)
- `src/state/manager.rs:291-339` - `handle_online_to_offline()` (49 строк)

**Решение**: Разбить на более мелкие функции

**Действия**:

1. Разбить `validate_streamer_login()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - `validate_not_empty()`
   - `validate_length()`
   - `validate_no_spaces()`
   - `validate_no_at_symbol()`
   - `validate_alphanumeric_underscore()`
2. Разбить `parse_and_validate_channel()` в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - `validate_username_channel()`
   - `validate_numeric_channel()`
3. Разбить `handle_offline_to_online()` в [`src/state/manager.rs`](src/state/manager.rs):
   - `set_online_pending_state()`
   - `spawn_grace_period_timer()`
4. Разбить `handle_online_to_offline()` в [`src/state/manager.rs`](src/state/manager.rs):
   - `set_offline_pending_state()`

**Риск**: Сложность понимания кода, трудности в поддержке

### 3.2 Вынесение числовых констант в конфиг

**Проблема**: В коде есть "магические числа":

- `src/state/models.rs:53` - `30` секунд для дубликатов
- `src/twitch/poller.rs:57` - `100` (размер чанка)
- `src/telegram/command_handler.rs:25` - `25` (максимальная длина username)

**Решение**: Вынести в конфиг

**Действия**:

1. Добавить поля в [`src/config.rs`](src/config.rs):
   - `duplicate_event_window_seconds`
   - `twitch_batch_size`
   - `max_username_length`
2. Использовать константы в коде вместо "магических чисел"
3. Добавить в `.env.example`

**Риск**: "Магические числа" затрудняют понимание и настройку

### 3.3 Вынесение сообщений в отдельный файл

**Проблема**: `handle_start()` и `handle_help()` возвращают большие строки с хардкодом.

**Решение**: Вынести в отдельный файл с шаблонами

**Действия**:

1. Создать [`src/telegram/templates.rs`](src/telegram/templates.rs) с `START_MESSAGE` и `HELP_MESSAGE`
2. Обновить [`src/telegram/mod.rs`](src/telegram/mod.rs) для экспорта templates
3. Обновить [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs) для использования templates

**Риск**: Хардкод сообщений затрудняет локализацию и изменение

### 3.4 Упрощение `handle_mysettings()`

**Проблема**: Функция содержит много логики форматирования ответа.

**Решение**: Вынести форматирование в отдельные методы

**Действия**:

1. Вынести форматирование в отдельные методы в [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - `format_settings_header()`
   - `format_settings_footer()`

**Риск**: Сложность понимания логики форматирования

## Порядок выполнения работ

### Этап 1: Создание миграций и обновление схемы БД (1 день)

1. Создать `migrations/002_stream_state.sql`
2. Обновить [`src/storage/sqlite.rs`](src/storage/sqlite.rs):
   - Добавить `get_target_chat_for_user()`
   - Добавить `get_all_stream_states()`
   - Добавить `save_stream_state()`
   - Обновить `UserSubscription` и `get_user_subscriptions()`
   - Обновить `get_all_twitch_user_ids()`
   - Обновить `get_streamer_configs()`

### Этап 2: Обновление конфигурации (1 день)

1. Обновить [`src/config.rs`](src/config.rs):
   - Добавить `admin_id`
   - Добавить `max_subscriptions_per_user`
   - Добавить `duplicate_event_window_seconds`
   - Добавить `twitch_batch_size`
   - Добавить `max_username_length`
2. Обновить `.env.example`

### Этап 3: Обновление state manager (2 дня)

1. Переписать [`src/state/manager.rs`](src/state/manager.rs):
   - Убрать HashMap
   - Использовать Storage
   - Добавить `restore_state()`
   - Переписать `handle_stream_online()` и `handle_stream_offline()`
   - Добавить методы обработки состояний
2. Обновить [`src/state/models.rs`](src/state/models.rs):
   - Добавить методы для работы с БД
   - Использовать константы из конфига

### Этап 4: Обновление Twitch API и poller (1 день)

1. Обновить [`src/twitch/api.rs`](src/twitch/api.rs):
   - Добавить глобальный rate limiter
   - Добавить метод `wait_for_rate_limit()`
2. Обновить [`src/twitch/poller.rs`](src/twitch/poller.rs):
   - Добавить batching
   - Получать `target_chat_id` для каждого стримера
   - Передавать в `handle_stream_online()`

### Этап 5: Обновление Telegram бота (1 день)

1. Обновить [`src/telegram/bot.rs`](src/telegram/bot.rs):
   - Добавить `send_with_retry()`
   - Обновить `send_stream_notification()` и `send_message()`
   - Добавить `notify_admin()`
2. Создать [`src/telegram/templates.rs`](src/telegram/templates.rs)
3. Обновить [`src/telegram/mod.rs`](src/telegram/mod.rs)

### Этап 6: Обновление command handler (2 дня)

1. Обновить [`src/telegram/command_handler.rs`](src/telegram/command_handler.rs):
   - Добавить проверку прав доступа
   - Обновить валидацию username
   - Обновить валидацию URL
   - Добавить проверку лимита подписок
   - Добавить обработку ошибок
   - Разбить функции на мелкие методы
   - Обновить `handle_start()` и `handle_help()` для использования templates
2. Обновить [`src/main.rs`](src/main.rs):
   - Вызвать `restore_state()` после инициализации

### Этап 7: Обновление зависимостей (1 день)

1. Обновить `Cargo.toml`:
   - Добавить `governor = "0.6"`
   - Добавить `nonzero_ext = "0.3"`
   - Добавить `url = "2.4"`

### Этап 8: Тестирование (2 дня)

1. Протестировать все критические сценарии:
   - Отправка уведомлений в несуществующий чат
   - Race condition при быстром перезапуске
   - Проверка прав доступа
   - Восстановление состояния после перезапуска
2. Протестировать rate limiting
3. Протестировать валидацию ввода
4. Протестировать лимиты подписок

## Итоговые результаты

### Безопасность

- ✅ Бот не упадет при отправке в несуществующий чат
- ✅ Защита от race conditions
- ✅ Проверка прав доступа
- ✅ Валидация ввода (username, URL)
- ✅ Ограничение на количество подписок

### Надежность

- ✅ Обработка сетевых ошибок с retry
- ✅ Rate limiting для Twitch API
- ✅ Сохранение состояния в БД
- ✅ Восстановление состояния после перезапуска
- ✅ Уведомления администратора об ошибках

### Производительность

- ✅ Batching запросов к Twitch API
- ✅ Глобальный rate limiter
- ✅ Оптимизированные SQL-запросы (GROUP BY вместо DISTINCT)
- ✅ Prepared statements

### Поддерживаемость

- ✅ Упрощенные функции (разбиты на мелкие методы)
- ✅ Вынесены константы в конфиг
- ✅ Вынесены сообщения в отдельный файл
- ✅ Устранено дублирование кода
- ✅ Четкая структура кода

## Оценка ресурсов

- **Время на разработку**: 10-12 дней
- **Сложность**: Высокая (требуется переписывание state manager)
- **Риски**:
  - Низкий: Потеря данных при миграции (можно восстановить из БД)
  - Средний: Сложность тестирования race conditions
  - Низкий: Совместимость с новыми зависимостями

## Рекомендации

1. **Начать с критических проблем** (Приоритет 1) - они влияют на стабильность бота
2. **Создать бэкап БД** перед миграцией
3. **Тестировать на staging-окружении** перед продакшеном
4. **Добавить мониторинг** после внедрения изменений
5. **Документировать изменения** в README.md
