CREATE TABLE prediction_schema (
    version INTEGER PRIMARY KEY CHECK (version = 1)
);
INSERT INTO prediction_schema(version) VALUES (1);

CREATE TABLE prediction_commands (
    guild_id TEXT NOT NULL CHECK (guild_id ~ '^[1-9][0-9]{0,19}$'),
    command_key TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    accepted_at BIGINT NOT NULL,
    response TEXT NOT NULL,
    last_revision BIGINT NOT NULL CHECK (last_revision >= 0),
    PRIMARY KEY (guild_id, command_key)
);

CREATE TABLE prediction_events (
    guild_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    command_key TEXT NOT NULL,
    accepted_at BIGINT NOT NULL,
    event JSONB NOT NULL CHECK (jsonb_typeof(event) = 'object'),
    event_source TEXT GENERATED ALWAYS AS (event->>'source') STORED NOT NULL,
    event_id UUID GENERATED ALWAYS AS ((event->>'id')::uuid) STORED NOT NULL,
    PRIMARY KEY (guild_id, revision),
    UNIQUE (event_source, event_id),
    FOREIGN KEY (guild_id, command_key) REFERENCES prediction_commands(guild_id, command_key)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (event->>'guildid' IS NOT NULL AND event->>'guildid' = guild_id),
    CHECK (event->>'revision' IS NOT NULL AND event->>'revision' = revision::text),
    CHECK (event->>'commandid' IS NOT NULL AND event->>'commandid' = command_key)
);

CREATE ROLE prediction_bot_runtime NOLOGIN;
GRANT USAGE ON SCHEMA public TO prediction_bot_runtime;
REVOKE ALL ON prediction_schema, prediction_commands, prediction_events FROM PUBLIC;
GRANT SELECT ON prediction_schema TO prediction_bot_runtime;
GRANT SELECT, INSERT ON prediction_commands, prediction_events TO prediction_bot_runtime;
