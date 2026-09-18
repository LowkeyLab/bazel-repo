CREATE TABLE prediction_announcement_settings (
  guild_id TEXT PRIMARY KEY CHECK (guild_id ~ '^[1-9][0-9]{0,19}$'),
  channel_id TEXT CHECK (channel_id ~ '^[1-9][0-9]{0,19}$'),
  enabled BOOLEAN NOT NULL DEFAULT FALSE,
  configuration_version BIGINT NOT NULL DEFAULT 0 CHECK (configuration_version >= 0),
  pause_reason TEXT,
  CHECK (NOT enabled OR channel_id IS NOT NULL)
);

CREATE TABLE prediction_announcement_outbox (
  guild_id TEXT NOT NULL,
  revision BIGINT NOT NULL,
  snapshot_version INTEGER NOT NULL CHECK (snapshot_version = 1),
  snapshot JSONB NOT NULL CHECK (jsonb_typeof(snapshot) = 'object'),
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'delivered', 'discarded')),
  attempts BIGINT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at BIGINT NOT NULL,
  last_failure TEXT,
  delivered_channel_id TEXT,
  delivered_message_id TEXT,
  PRIMARY KEY (guild_id, revision),
  FOREIGN KEY (guild_id, revision) REFERENCES prediction_events(guild_id, revision),
  CHECK (state <> 'delivered' OR
    (delivered_channel_id IS NOT NULL AND delivered_message_id IS NOT NULL))
);

CREATE INDEX prediction_announcement_due
  ON prediction_announcement_outbox(next_attempt_at, guild_id, revision)
  WHERE state = 'pending';

REVOKE ALL ON prediction_announcement_settings, prediction_announcement_outbox FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON prediction_announcement_settings,
  prediction_announcement_outbox TO prediction_bot_runtime;
GRANT SELECT ON _sqlx_migrations TO prediction_bot_runtime;
