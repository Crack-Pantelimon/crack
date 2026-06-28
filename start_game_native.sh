#!/bin/bash
set -ex

export RUST_LOG=info
export WGPU_BACKEND=gl

cd crack_demo/demo_resolution_selector_web_bevy
cargo run