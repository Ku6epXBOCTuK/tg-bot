# Twitch Telegram Bot

Production-ready Telegram bot for tracking Twitch streams with grace periods. The bot monitors stream start/stop events via Twitch EventSub webhooks and sends notifications to a Telegram channel.

## Features

- ✅ **Twitch EventSub Integration**: Real-time stream monitoring via webhooks
- ✅ **Grace Period System**: Filters out restarts and brief interruptions
  - 3 minutes grace period for stream start
  - 10 minutes grace period for stream end
- ✅ **Automatic Message Management**: Sends notifications on start, deletes on end
- ✅ **State Persistence**: SQLite database for surviving restarts
- ✅ **Duplicate Event Protection**: Prevents processing duplicate webhook events
- ✅ **Secure Webhook Verification**: HMAC signature validation
- ✅ **Error Handling**: Retry logic for Telegram API calls
- ✅ **Graceful Shutdown**: Proper cleanup on termination

## Architecture

```
src/
├── main.rs              # Application entry point
├── config.rs            # Configuration management
├── twitch/
│   ├── mod.rs
│   ├── eventsub.rs     # EventSub models & verification
│   └── api.rs          # Twitch API client
├── telegram/
│   ├── mod.rs
│   └── bot.rs          # Telegram bot integration
├── state/
│   ├── mod.rs
│   ├── models.rs       # State machine models
│   └── manager.rs      # Grace period logic
├── web/
│   ├── mod.rs
│   └── routes.rs       # Axum web server
└── storage/
    ├── mod.rs
    └── sqlite.rs       # Database operations
```

## State Machine

```
Offline → OnlinePending → Online → OfflinePending → Offline
    ↑          ↑              ↑           ↑
    └──────────┴──────────────┴───────────┘
```

- **OnlinePending**: Waiting 3 minutes before confirming stream start
- **Online**: Stream confirmed, notification sent
- **OfflinePending**: Waiting 10 minutes before deleting notification
- **Offline**: Stream confirmed ended

## Installation

### Prerequisites

- Rust 1.70+
- SQLite 3
- Telegram Bot Token
- Twitch Application (Client ID & Secret)
- Public HTTPS URL for webhooks

### Setup

1. **Clone and build**:

```bash
git clone <repository>
cd twitch-telegram-bot
cargo build --release
```

2. **Configure environment**:

```bash
cp .env.example .env
# Edit .env with your credentials
```

3. **Initialize database**:

The bot will automatically create the database file and apply migrations on first run. No manual setup required!

4. **Get Twitch credentials**:
   - Go to https://dev.twitch.tv/console/apps
   - Create new application
   - Note Client ID and generate Client Secret

5. **Get Telegram credentials**:
   - Talk to @BotFather on Telegram
   - Create new bot
   - Note the bot token
   - Add bot to your channel as admin
   - Get channel ID (use @userinfobot or similar)

6. **Run the bot**:

```bash
cargo run --release
```

## Configuration

### Environment Variables

| Variable               | Description                      | Required | Default                |
| ---------------------- | -------------------------------- | -------- | ---------------------- |
| `TELEGRAM_BOT_TOKEN`   | Telegram bot token               | Yes      | -                      |
| `TELEGRAM_CHANNEL_ID`  | Telegram channel ID (negative)   | Yes      | -                      |
| `TWITCH_CLIENT_ID`     | Twitch application client ID     | Yes      | -                      |
| `TWITCH_CLIENT_SECRET` | Twitch application client secret | Yes      | -                      |
| `WEBHOOK_BASE_URL`     | Public URL of your bot           | Yes      | -                      |
| `WEBHOOK_SECRET`       | Secret for webhook verification  | Yes      | -                      |
| `DATABASE_URL`         | SQLite database path             | No       | `sqlite:twitch_bot.db` |
| `SERVER_PORT`          | HTTP server port                 | No       | `8080`                 |
| `GRACE_PERIOD_ONLINE`  | Online grace period (seconds)    | No       | `180`                  |
| `GRACE_PERIOD_OFFLINE` | Offline grace period (seconds)   | No       | `600`                  |

### Webhook URL

Your `WEBHOOK_BASE_URL` must be publicly accessible and support HTTPS. The bot will listen on:

- `GET {WEBHOOK_BASE_URL}/webhook/twitch` - Verification endpoint
- `POST {WEBHOOK_BASE_URL}/webhook/twitch` - Event notifications

## Usage

### Adding Streamers

Currently, streamers need to be added manually to the database. Future versions will include bot commands.

```sql
INSERT INTO streamers (streamer_id, streamer_login, streamer_name)
VALUES ('123456789', 'streamer_login', 'Streamer Name');
```

The bot will automatically:

1. Create EventSub subscriptions for `stream.online` and `stream.offline`
2. Monitor the stream
3. Send notifications to your Telegram channel

### Monitoring

Check logs for:

- Webhook reception and verification
- State transitions
- Telegram message operations
- Error handling and retries

## Deployment

### Railway.app (Recommended)

1. Create new project
2. Connect GitHub repository
3. Add environment variables
4. Deploy
5. Set webhook URL in Railway to your app URL

### Docker

```dockerfile
FROM rust:1.70-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y sqlite3
COPY --from=builder /app/target/release/twitch-telegram-bot /usr/local/bin
COPY migrations /app/migrations
WORKDIR /app
CMD ["twitch-telegram-bot"]
```

### Systemd Service

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

## Error Handling

The bot handles:

- **Network failures**: Automatic retries with exponential backoff
- **Telegram API errors**: 3 retries with 2-second delays
- **Invalid webhooks**: Returns 401/400 status codes
- **Database errors**: Logs and continues
- **Graceful shutdown**: Cleans up resources on SIGTERM/SIGINT

## Security

- **Webhook signatures**: All webhooks are verified using HMAC-SHA256
- **Environment variables**: Secrets stored securely
- **Input validation**: All webhook data is validated
- **SQL injection protection**: Using prepared statements

## Troubleshooting

### Webhook verification fails

- Check `WEBHOOK_SECRET` matches in bot and Twitch console
- Verify `WEBHOOK_BASE_URL` is correct and accessible
- Check HTTPS certificate

### Telegram messages not sending

- Verify bot token and channel ID
- Ensure bot is admin in channel
- Check channel privacy settings

### Stream events not received

- Verify EventSub subscriptions exist in Twitch console
- Check webhook URL is publicly accessible
- Review logs for subscription creation errors

### Database locked errors

- Ensure only one instance is running
- Check file permissions on database file

### Database not created

- The bot automatically creates the database on first run
- Ensure DATABASE_URL is set correctly in .env
- Check write permissions in the project directory

## Development

### Running tests

```bash
cargo test
```

### Adding new features

1. Update models in `state/models.rs`
2. Add handlers in `state/manager.rs`
3. Update webhook processing in `web/routes.rs`

### Code structure

- All async operations use Tokio
- State management is thread-safe with RwLock
- Web server uses Axum
- Database uses SQLx with connection pooling

## License

MIT License - feel free to use and modify.

## Support

For issues and questions, please check:

1. Logs for detailed error messages
2. Twitch EventSub documentation
3. Telegram Bot API documentation
4. This README for configuration issues
