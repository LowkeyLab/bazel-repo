CREATE TABLE IF NOT EXISTS servers (
    id UUID PRIMARY KEY,
    discord_server BIGINT NOT NULL UNIQUE,
    display_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
