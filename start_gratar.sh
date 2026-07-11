#!/bin/bash
set -ex

export RUST_LOG=info

# Get absolute path to the project root directory where env.sh is located
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Set ALSA/udev package config paths for native compilation in the sandbox
export PKG_CONFIG_PATH="$ROOT_DIR/scratch/alsa_dev/usr/lib/x86_64-linux-gnu/pkgconfig"

cd "$ROOT_DIR"
exec "$ROOT_DIR/env.sh" cargo run --package demo_resolution_selector_web_bevy --bin gratar
