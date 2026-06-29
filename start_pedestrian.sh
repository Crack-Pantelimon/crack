#!/bin/bash
set -ex

if ! ./env.sh rustup show active-toolchain >/dev/null 2>&1; then
  ./env.sh rustup default stable
fi

./env.sh cargo run --bin pedestrian_animations --package demo_resolution_selector_web_bevy