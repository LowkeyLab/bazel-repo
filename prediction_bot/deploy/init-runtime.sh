#!/bin/sh
set -eu

: "${POSTGRES_RUNTIME_PASSWORD:?POSTGRES_RUNTIME_PASSWORD is required}"

psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
	--set=ON_ERROR_STOP=1 --set=runtime_password="$POSTGRES_RUNTIME_PASSWORD" <<'SQL'
CREATE ROLE prediction_bot_runtime NOLOGIN;
CREATE ROLE prediction_bot_app LOGIN PASSWORD :'runtime_password';
GRANT prediction_bot_runtime TO prediction_bot_app;
SQL
