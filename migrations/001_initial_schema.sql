-- Create streamers table
CREATE TABLE
    IF NOT EXISTS streamers (
        streamer_id TEXT PRIMARY KEY,
        streamer_login TEXT NOT NULL UNIQUE,
        streamer_name TEXT NOT NULL,
        created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
        eventsub_subscription_id TEXT,
        online_subscription_id TEXT,
        offline_subscription_id TEXT
    );

-- Create stream_states table
CREATE TABLE
    IF NOT EXISTS stream_states (
        streamer_id TEXT PRIMARY KEY,
        streamer_login TEXT NOT NULL,
        streamer_name TEXT NOT NULL,
        status TEXT NOT NULL,
        started_at TIMESTAMP,
        pending_started_at TIMESTAMP,
        telegram_message_id INTEGER,
        last_event_id TEXT,
        last_event_timestamp TIMESTAMP,
        grace_period_start TIMESTAMP,
        FOREIGN KEY (streamer_id) REFERENCES streamers (streamer_id) ON DELETE CASCADE
    );

-- Create user_subscriptions table
CREATE TABLE
    IF NOT EXISTS user_subscriptions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_telegram_id BIGINT NOT NULL,
        twitch_user_id TEXT NOT NULL,
        UNIQUE (user_telegram_id, twitch_user_id)
    );

-- Create notification_settings table
CREATE TABLE
    IF NOT EXISTS notification_settings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        subscription_id INTEGER NOT NULL REFERENCES user_subscriptions (id) ON DELETE CASCADE,
        target_chat_id TEXT NOT NULL,
        custom_message TEXT NOT NULL DEFAULT '🔴 {streamer} начал стрим!',
        inline_buttons_json TEXT,
        UNIQUE (subscription_id)
    );

-- Create indexes for better performance
CREATE INDEX IF NOT EXISTS idx_streamers_login ON streamers (streamer_login);

CREATE INDEX IF NOT EXISTS idx_stream_states_status ON stream_states (status);

CREATE INDEX IF NOT EXISTS idx_user_subscriptions_user ON user_subscriptions (user_telegram_id);

CREATE INDEX IF NOT EXISTS idx_user_subscriptions_twitch ON user_subscriptions (twitch_user_id);