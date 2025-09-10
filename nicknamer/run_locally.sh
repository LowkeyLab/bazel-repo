#!/bin/bash
# Exit immediately if a command exits with a non-zero status.
set -e

# Define the path to our binary and compose file.
# Bazel places `data` dependencies in the same directory as the script.
APP_BINARY="nicknamer/server/server"
COMPOSE_FILE="nicknamer/compose.yaml"

# Function to clean up containers on exit
cleanup() {
  echo "--- Shutting down Docker Compose services ---"
  docker compose -f "$COMPOSE_FILE" down
}

# Use `trap` to ensure the cleanup function is called when the script exits,
# whether it's from finishing, being interrupted (Ctrl+C), or an error.
trap cleanup EXIT

echo "--- Starting Docker Compose services in the background ---"
docker compose -f "$COMPOSE_FILE" up -d

# Best practice: Wait for the service to be healthy. A simple sleep is
# often not robust enough. Here we poll until Postgres is ready.
echo "--- Waiting for Postgres to be ready ---"
until docker exec -it "$(docker compose -f "$COMPOSE_FILE" ps -q postgres)" pg_isready -U user; do
  echo "Postgres is not ready yet. Waiting..."
  sleep 2
done


echo "--- Starting the application ---"
# Execute the actual application binary
$APP_BINARY