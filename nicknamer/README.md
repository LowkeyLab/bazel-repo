# Nicknamer Server

This directory contains the nicknamer server application.

## Building and Running

### Local Development

To run the server locally with dependencies:

```bash
bazel run //nicknamer:run_locally
```

### Docker Image

#### Building the Docker Image

To build the nicknamer server as a Docker image:

```bash
# Build the image (convenience alias)
bazel build //nicknamer:build_image

# Or using the direct target
bazel build //nicknamer/server/bin:image

# Or using the full target name
bazel build //nicknamer/server/bin:nicknamer_image
```

#### Pushing to GitHub Container Registry

To push the image to GitHub's Container Registry (ghcr.io):

```bash
# Make sure you're logged in to ghcr.io
docker login ghcr.io

# Push the image (convenience alias)
bazel run //nicknamer:push_image

# Or using the direct target
bazel run //nicknamer/server/bin:push_image
```

**Note**: You'll need to have proper authentication set up for GitHub Container Registry. Make sure you have:

1. A GitHub Personal Access Token with `write:packages` permission
2. Docker logged in to ghcr.io: `echo $GITHUB_TOKEN | docker login ghcr.io -u <username> --password-stdin`

#### Using the Built Image

After building, you can load the image into your local Docker daemon:

```bash
# Build and load the image
bazel run //nicknamer/server/bin:nicknamer_image
docker run --rm -p 8080:8080 nicknamer-server:latest
```

## Project Structure

- `server/bin/` - Main server binary
- `server/lib/` - Server library code
- `migration/` - Database migration utilities
- `compose.yaml` - Docker Compose configuration for local development
