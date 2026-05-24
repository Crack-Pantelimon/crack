#!/bin/bash
set -ex

# ./build_worker.sh

export RUST_LOG=info

cargo test  --package thread_crackworker 
cargo test --package _crack_utils