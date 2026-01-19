# Deployment Guide

## Railway.app (Recommended)

1. Fork this repository
2. Create new project on Railway
3. Connect GitHub repository
4. Add environment variables in Railway dashboard
5. Deploy automatically

## Docker

```bash
docker build -t twitch-telegram-bot .
docker run -d --env-file .env twitch-telegram-bot
```

## Systemd (Linux)

Create `/etc/systemd/system/twitch-telegram-bot.service`:

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
sudo systemctl enable twitch-telegram-bot
sudo systemctl start twitch-telegram-bot
sudo systemctl status twitch-telegram-bot
```
