#!/bin/bash

set -ex
export IMG_NAME=crack-dev:latest

if ! ( docker volume ls | grep crack-dev-root-dir ) ; then 
    docker volume create crack-dev-root-dir
fi

if ! ( docker volume ls | grep crack-dev-target-dir ) ; then 
    docker volume create crack-dev-target-dir
fi

# Shared, non-overlaid volume that holds ALL mutable harness state. It is mounted
# read-write into crack-dev AND (later plans) into every sandbox, so the server and
# the sandboxed pi processes share one stable view that is never an overlay lower.
if ! docker volume ls | grep -q crack-harness-data; then
    docker volume create crack-harness-data
fi
# Anchor container: keeps the volume referenced and gives a stable target for
# inspection/backup (`docker exec crack-harness-data ls /crack-harness-data`).
if ! docker ps -a --format '{{.Names}}' | grep -qx crack-harness-data; then
    docker run -d --name crack-harness-data --restart unless-stopped \
        -v crack-harness-data:/crack-harness-data \
        "$IMG_NAME" sleep infinity
fi

docker rm -f crack-dev || true
# ./build.sh

# Docker-out-of-docker: mount the host's rootless podman API socket so crack-dev
# can spawn sibling sandbox containers on the host. CRACK_HOST_REPO_ROOT is the
# HOST path of the repo (crack-dev sees it at /workspace) — sandbox :O overlays
# must reference host paths, not container paths, since host podman resolves them.
HOST_PODMAN_SOCK="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock"
systemctl --user enable --now podman.socket || true

# Shared network: sandboxes reach crack-server as http://crack-dev:9847
if ! docker network exists crack-net; then
    docker network create crack-net
fi

docker run -d \
  --name crack-dev \
  --network crack-net \
  --network-alias crack-dev \
  -v "$(dirname $PWD):/workspace" \
  -v "crack-dev-root-dir:/root" \
  -v "crack-dev-target-dir:/workspace/target" \
  -v "crack-harness-data:/crack-harness-data" \
  -v "${HOST_PODMAN_SOCK}:/run/podman/podman.sock" \
  -e "CONTAINER_HOST=unix:///run/podman/podman.sock" \
  -e "CRACK_HOST_REPO_ROOT=$(dirname $PWD)" \
  -e "CRACK_HARNESS_DATA_DIR=/crack-harness-data" \
  -p "127.0.0.1:9847:9847" \
  -p "127.0.0.1:21122:22" \
  -p "127.0.0.1:9930:9930" \
  -p "127.0.0.1:9931:9931" \
  -p "127.0.0.1:9932:9932" \
  -p "127.0.0.1:9877:9877" \
  --init \
  $IMG_NAME /bin/bash _docker/_cont_start.sh