#!/bin/bash

set -ex


cd /workspace/.pi/crack/server
uv sync
export CRACK_PI_PROJECT_ROOT=/workspace
uv run crack-server 