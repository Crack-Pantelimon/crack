#!/bin/bash

set -ex

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Docker-out-of-docker: mount the host's rootless podman API socket so crack-dev
# can spawn sibling sandbox containers on the host.
HOST_PODMAN_SOCK="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock"
systemctl --user enable --now podman.socket || true

export HOST_PODMAN_SOCK
export CRACK_HOST_REPO_ROOT="$REPO_ROOT"

docker network inspect crack-docker-net >/dev/null 2>&1 \
  || docker network create crack-docker-net

COMPOSE=(docker compose -f "$SCRIPT_DIR/docker-compose.yml")

# Bring up only what isn't already running; never disturb live infra.
"${COMPOSE[@]}" up -d --no-recreate

# Always cycle crack-dev so code/_cont_start.sh changes take effect,
# without touching its (formerly depends_on) infra siblings.
"${COMPOSE[@]}" up -d --force-recreate --no-deps crack-dev

# Graphiti MCP + FalkorDB browser bind-mount entrypoints/patches and env; recreate
# so those take effect without bouncing milvus/ollama/falkordb data planes.
"${COMPOSE[@]}" up -d --force-recreate --no-deps graphiti-mcp falkordb-browser
