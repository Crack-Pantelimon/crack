#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
blender -b -P street_cleanup/render_top_down.py -- street_cleanup/demo/demo_batch.json
