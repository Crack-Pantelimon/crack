#!/bin/bash
set -ex

# ./build_worker.sh

export RUST_LOG=info

killall cargo-watch || true
cargo watch \
    --why \
    --watch packages/ \
    --watch Cargo.toml \
    --watch Cargo.lock \
    --watch src/ \
    --watch crack_demo/web_worker \
    -s "./build_worker.sh" 
