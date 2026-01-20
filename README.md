# Twitch Telegram Bot

Telegram bot для отслеживания стримов на Twitch через опрос API.

## Features

- ✅ Опрос Twitch API (периодический)
- ✅ Грейс-периоды (3 мин онлайн, 10 мин офлайн)
- ✅ Персональные настройки для каждого пользователя
- ✅ Кастомные сообщения и кнопки
- ✅ SQLite база данных (автоматическое создание)
- ✅ Обработка ошибок и ретраи

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Configure

```bash
cp .env.example .env
# Edit .env with your credentials
```

### 3. Run

```bash
cargo run --release
```

База данных создается автоматически!

### 4. Commands

- `/add <twitch_login>` - Добавить стримера
- `/set_channel <@username или ID>` - Куда отправлять уведомления
- `/set_text <текст>` - Кастомный текст уведомления
- `/add_button <текст> | <url>` - Добавить кнопку
- `/test` - Тестовое уведомление
- `/preview` - Превью в личку
- `/my_settings` - Ваши подписки
- `/remove <twitch_login>` - Удалить стримера
- `/help` - Справка

## Environment Variables

| Variable                   | Required | Description                                 |
| -------------------------- | -------- | ------------------------------------------- |
| `TELEGRAM_BOT_TOKEN`       | Yes      | Токен бота                                  |
| `TWITCH_CLIENT_ID`         | Yes      | Twitch Client ID                            |
| `TWITCH_CLIENT_SECRET`     | Yes      | Twitch Client Secret                        |
| `DATABASE_URL`             | No       | Путь к БД (default: `sqlite:twitch_bot.db`) |
| `GRACE_PERIOD_ONLINE`      | No       | Грейс-период онлайн (default: `180`)        |
| `GRACE_PERIOD_OFFLINE`     | No       | Грейс-период офлайн (default: `600`)        |
| `POLLING_INTERVAL_SECONDS` | No       | Интервал опроса (default: `60`)             |

## Deployment

### Docker

```bash
docker build -t twitch-telegram-bot .
docker run -d --env-file .env twitch-telegram-bot
```

### Systemd

```ini
[Unit]
Description=Twitch Telegram Bot
After=network.target

[Service]
Type=simple
User=botuser
WorkingDirectory=/opt/twitch-bot
EnvironmentFile=/opt/twitch-bot/.env
ExecStart=/opt/twitch-bot/target/release/twitch-telegram-bot
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

## Troubleshooting

### База данных не создается

- Проверьте `DATABASE_URL` в `.env`
- Права на запись в директорию

### Уведомления не отправляются

- Проверьте `TELEGRAM_BOT_TOKEN`
- Добавьте бота в канал как администратора
- Для каналов используйте ID вместо @username

### Стримы не отслеживаются

- Проверьте `TWITCH_CLIENT_ID` и `TWITCH_CLIENT_SECRET`
- Убедитесь, что бот запущен постоянно
- Проверьте логи на ошибки API

## Architecture

```
Telegram Bot → State Manager → Twitch API Poller
                    ↓
              SQLite Database
```

**State Machine:**

```
Offline → OnlinePending (3 min) → Online → OfflinePending (10 min) → Offline
```

## License

MIT
