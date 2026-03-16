# Add Server Feature — Design Spec

## Overview

Add support for creating Discord servers as first-class entities in nicknamer2, both backend (GraphQL mutation, database table, domain model) and frontend (form UI).

## Database

New migration `004_create_servers_table.sql`:

```sql
CREATE TABLE IF NOT EXISTS servers (
    id UUID PRIMARY KEY,
    discord_server BIGINT NOT NULL UNIQUE,
    display_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

No foreign key from `names` to `servers`. The `names` table stays unchanged.

## Backend Domain

New `server/` module (sibling to `name/`):

- **Domain struct:** `Server { id: DiscordServerId, display_name: String, created_at, updated_at }`
- **Repository traits:** `ServerCreator` (save), `ServerReader` (get by discord_server, list with cursor pagination)
- **Repo implementation:** PostgreSQL via sqlx, same patterns as `name/repo.rs`
- **Service:** `create_server(discord_server_id, display_name)` — validates ID, delegates to repo, errors on duplicate

The existing `name/` module's `list_servers` repo method (which derives servers from distinct `discord_server` in `names`) is replaced to query the new `servers` table.

## GraphQL

### New mutation: `createServer`

- **Input:** `CreateServerInput { clientMutationId?: String, discordServerId: String, displayName: String }`
- **Output:** `CreateServerPayload { clientMutationId?: String, server: Server }`
- **Behavior:** Requires auth, validates discordServerId as u64 > 0, returns error if server already exists

### Updated `Server` type

- New field: `displayName: String!`
- Existing fields unchanged: `id`, `serverId`, `names` connection

### Updated queries

- `servers` and `server(id:)` query the new `servers` table instead of deriving from `names`

## Frontend

### New component: `AddServerComponent`

- **Route:** `/servers/new`
- **UI:** DaisyUI form with Discord Server ID + Display Name inputs, submit button, error/success alerts
- **Pattern:** Signals for state, `inject()` for DI, `OnPush` change detection
- **On success:** Navigate to `/servers/:serverId/names`

### Updated `ServerListComponent`

- Add "Add Server" button linking to `/servers/new`

### New GraphQL operation

- `create-server.graphql` mutation file
- Codegen generates `CreateServerGQL` Apollo service

## Auth

All mutations require authentication (consistent with existing `createName` pattern).
