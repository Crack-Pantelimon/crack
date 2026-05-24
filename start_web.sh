#!/bin/bash
set -ex

# ./build_worker.sh

export RUST_LOG=info

killall cargo-watch || true
bash -c 'cargo watch --watch packages/ --watch Cargo.toml --watch Cargo.lock --watch src/ -s "./build_worker.sh && dx serve --keep-names  --package web_frontend"  '
