-- Drop old tables and recreate names without user foreign key
DROP TABLE IF EXISTS names;
DROP TABLE IF EXISTS users;

CREATE TABLE IF NOT EXISTS names (
    id UUID PRIMARY KEY,
    discord_id BIGINT NOT NULL,
    discord_server BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(discord_id, discord_server)
);
