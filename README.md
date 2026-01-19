# Twitch Telegram Bot

Production-ready Telegram bot for tracking Twitch streams with grace periods.

## Features

- ✅ Twitch EventSub webhook integration
- ✅ Grace period system (3 min online, 10 min offline)
- ✅ Automatic Telegram notifications
- ✅ SQLite database persistence
- ✅ Automatic database creation
- ✅ Secure webhook verification
- ✅ Error handling and retries

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

Database will be created automatically!

### 4. Add streamers

```sql
INSERT INTO streamers (streamer_id, streamer_login, streamer_name)
VALUES ('123456789', 'streamer_login', 'Streamer Name');
```

## Environment Variables

| Variable               | Required | Description                                            |
| ---------------------- | -------- | ------------------------------------------------------ |
| `TELEGRAM_BOT_TOKEN`   | Yes      | Telegram bot token                                     |
| `TELEGRAM_CHANNEL_ID`  | Yes      | Telegram channel ID (negative)                         |
| `TWITCH_CLIENT_ID`     | Yes      | Twitch application client ID                           |
| `TWITCH_CLIENT_SECRET` | Yes      | Twitch application client secret                       |
| `WEBHOOK_BASE_URL`     | Yes      | Public HTTPS URL of your bot                           |
| `WEBHOOK_SECRET`       | Yes      | Secret for webhook verification                        |
| `DATABASE_URL`         | No       | SQLite database path (default: `sqlite:twitch_bot.db`) |
| `SERVER_PORT`          | No       | HTTP server port (default: `8080`)                     |
| `GRACE_PERIOD_ONLINE`  | No       | Online grace period in seconds (default: `180`)        |
| `GRACE_PERIOD_OFFLINE` | No       | Offline grace period in seconds (default: `600`)       |

## Deployment

### Railway.app (Recommended)

1. Fork this repository
2. Create new project on Railway
3. Connect GitHub repository
4. Add environment variables
5. Deploy

### Docker

```bash
docker build -t twitch-telegram-bot .
docker run -d --env-file .env twitch-telegram-bot
```

### Systemd (Linux)

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

### Database not created

- Check `DATABASE_URL` in `.env`
- Ensure you have write permissions
- Use simple path: `sqlite:twitch_bot.db`

### Telegram messages not sending

- Verify `TELEGRAM_BOT_TOKEN` is correct
- Verify `TELEGRAM_CHANNEL_ID` is correct (must be negative)
- Ensure bot has permission to post in channel

### Webhooks not received

- `WEBHOOK_BASE_URL` must be public HTTPS URL
- Twitch requires HTTPS for webhooks
- For local testing, use ngrok: `ngrok http 8080`

## Architecture

```
Offline → OnlinePending (3 min) → Online → OfflinePending (10 min) → Offline
```

## License

MIT
