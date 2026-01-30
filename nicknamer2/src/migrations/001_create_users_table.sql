-- Create users table for nicknamer2
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    discord_id BIGINT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
