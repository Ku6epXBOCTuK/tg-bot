# Deployment Guide

## Quick Start

### 1. Set up environment variables

Create a `.env` file in the project root:

```bash
cp .env.example .env
# Edit .env with your credentials
```

### 2. Build the project

```bash
cargo build --release
```

### 3. Run the bot

```bash
cargo run --release
```

## Database Setup

The bot uses SQLite and will automatically create the database file and apply migrations on first run. The database schema is managed by SQLx migrations located in the `migrations/` directory.

## Testing the Bot

### Manual Testing

1. **Start the bot**:

   ```bash
   cargo run --release
   ```

2. **Test webhook endpoint**:

   ```bash
   curl -X GET http://localhost:8080/webhook/twitch?hub.challenge=test
   ```

3. **Simulate a webhook event** (requires proper signature):
   ```bash
   curl -X POST http://localhost:8080/webhook/twitch \
     -H "Twitch-Eventsub-Message-Signature: sha256=<signature>" \
     -H "Twitch-Eventsub-Message-Id: test-id" \
     -H "Twitch-Eventsub-Message-Timestamp: 2024-01-01T00:00:00Z" \
     -H "Twitch-Eventsub-Message-Type: notification" \
     -H "Twitch-Eventsub-Subscription-Type: stream.online" \
     -H "Twitch-Eventsub-Subscription-Version: 1" \
     -d '{"subscription":{"id":"test","type":"stream.online"},"event":{"broadcaster_user_id":"123","broadcaster_user_login":"test","broadcaster_user_name":"Test","id":"456","type":"live","started_at":"2024-01-01T00:00:00Z"}}'
   ```

### Integration Testing

1. **Add a streamer to the database**:

   ```sql
   INSERT INTO streamers (streamer_id, streamer_login, streamer_name)
   VALUES ('123456789', 'test_streamer', 'Test Streamer');
   ```

2. **Check logs** for EventSub subscription creation:

   ```
   INFO Created stream.online subscription for user_id 123456789: sub-123
   INFO Created stream.offline subscription for user_id 123456789: sub-456
   ```

3. **Simulate stream events** using the webhook endpoint

4. **Verify Telegram notifications** are sent to your channel

## Grace Period Testing

### Test Online Grace Period (3 minutes)

1. Send `stream.online` event
2. Wait 2 minutes - bot should be in `OnlinePending` state
3. Send `stream.offline` event - bot should cancel notification
4. Send `stream.online` event again
5. Wait 3+ minutes - bot should send Telegram notification

### Test Offline Grace Period (10 minutes)

1. Send `stream.online` event
2. Wait 3+ minutes - bot sends notification
3. Send `stream.offline` event
4. Wait 5 minutes - bot should be in `OfflinePending` state
5. Send `stream.online` event - bot should cancel deletion
6. Send `stream.offline` event again
7. Wait 10+ minutes - bot should delete Telegram notification

## Production Deployment

### Railway.app

1. Create new project
2. Connect GitHub repository
3. Add environment variables in Railway dashboard
4. Deploy
5. Set webhook URL in Twitch Developer Console to: `https://your-app.railway.app/webhook/twitch`

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

### Systemd

Create `/etc/systemd/system/twitch-bot.service`:

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

Enable and start:

```bash
sudo systemctl enable twitch-bot
sudo systemctl start twitch-bot
sudo systemctl status twitch-bot
```

## Monitoring

### View logs

```bash
# Systemd
sudo journalctl -u twitch-bot -f

# Docker
docker logs -f twitch-bot-container

# Manual
RUST_LOG=info cargo run --release
```

### Check database state

```bash
sqlite3 twitch_bot.db
```

```sql
-- View all streamers
SELECT * FROM streamers;

-- View current stream states
SELECT * FROM stream_states;

-- View EventSub subscriptions
SELECT * FROM streamers WHERE online_subscription_id IS NOT NULL;
```

## Troubleshooting

### Webhook verification fails

**Symptoms**: Twitch console shows "Failed to verify webhook"

**Solutions**:

1. Check `WEBHOOK_SECRET` matches in bot and Twitch console
2. Verify `WEBHOOK_BASE_URL` is correct and publicly accessible
3. Ensure HTTPS certificate is valid
4. Check logs for signature verification errors

### Telegram messages not sending

**Symptoms**: No messages in channel, logs show Telegram errors

**Solutions**:

1. Verify bot token: `TELEGRAM_BOT_TOKEN`
2. Check channel ID: `TELEGRAM_CHANNEL_ID` (must be negative for channels)
3. Ensure bot is admin in the channel
4. Check channel privacy settings (public channels work best)

### Stream events not received

**Symptoms**: Stream starts/stops but no notifications

**Solutions**:

1. Check Twitch console for active EventSub subscriptions
2. Verify webhook URL is accessible from internet
3. Review logs for subscription creation errors
4. Ensure streamer is added to database

### Database locked errors

**Symptoms**: "database is locked" errors in logs

**Solutions**:

1. Ensure only one instance is running
2. Check file permissions on database file
3. Reduce concurrent operations if needed

### Grace periods not working

**Symptoms**: Notifications sent immediately or not at all

**Solutions**:

1. Check `GRACE_PERIOD_ONLINE` and `GRACE_PERIOD_OFFLINE` values
2. Verify system clock is correct
3. Check logs for state transitions
4. Ensure no duplicate events are being processed

## Performance Tuning

### Database

- SQLite is suitable for small to medium deployments
- For high traffic, consider PostgreSQL
- Regular backups recommended

### Memory

- State is kept in memory for fast access
- Database is used for persistence
- Typical memory usage: 50-100MB

### Concurrency

- Tokio runtime handles all async operations
- Default thread count is CPU cores
- Can be tuned via `tokio::runtime::Builder`

## Security Checklist

- [ ] Use strong `WEBHOOK_SECRET` (32+ random characters)
- [ ] Store secrets in environment variables, not code
- [ ] Use HTTPS for webhook URL
- [ ] Restrict database file permissions (600)
- [ ] Run bot as non-root user
- [ ] Keep dependencies updated
- [ ] Monitor logs for suspicious activity
- [ ] Use firewall to restrict access if needed

## Maintenance

### Regular tasks

1. **Backup database**:

   ```bash
   cp twitch_bot.db twitch_bot.db.backup.$(date +%Y%m%d)
   ```

2. **Update dependencies**:

   ```bash
   cargo update
   cargo build --release
   ```

3. **Clean old logs**:
   ```bash
   # If using file logging
   find /var/log/twitch-bot -name "*.log" -mtime +30 -delete
   ```

### Upgrading

1. Stop the bot
2. Backup database
3. Pull latest changes
4. Run `cargo build --release`
5. Start the bot
6. Verify EventSub subscriptions are recreated

## Support

For issues:

1. Check logs with `RUST_LOG=debug`
2. Review this documentation
3. Check Twitch EventSub status: https://dev.twitch.tv/console/eventsub
4. Verify Telegram bot is working: https://t.me/your_bot_username

## Metrics to Monitor

- **Webhook delivery rate**: Should be near 100%
- **Telegram success rate**: Should be near 100%
- **Memory usage**: Should be stable
- **Database size**: Should grow slowly
- **EventSub subscription age**: Renew every 10 days
