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

# docker compose build
docker compose -f "$SCRIPT_DIR/docker-compose.yml" up -d --remove-orphans
