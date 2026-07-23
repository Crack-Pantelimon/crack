#!/bin/bash

set -ex
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export IMG_NAME=localhost/crack-dev:latest

docker build -t "$IMG_NAME" -f "$SCRIPT_DIR/image/Dockerfile" "$SCRIPT_DIR/image/"
