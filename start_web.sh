#!/bin/bash
set -ex

# ./build_worker.sh

export RUST_LOG=info

dx serve --keep-names  --package web_frontend