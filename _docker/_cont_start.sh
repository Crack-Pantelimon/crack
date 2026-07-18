#!/bin/bash

set -ex


cd /workspace/.pi/crack/server
uv sync
export CRACK_PI_PROJECT_ROOT=/workspace
mkdir -p /workspace/.pi/crack/harness
# Single-instance, auto-refreshing worker: if the flock is already held, exit 0.
( flock -n /workspace/.pi/crack/harness/worker.lock uv run crack-worker || true ) &
uv run crack-server