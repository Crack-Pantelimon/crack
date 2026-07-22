#!/bin/bash
# Cheap sandbox entrypoint: shared env + lazy MCP config, no eager HTTP bridges or crack-server.
set -ex

source /workspace/_docker/_sandbox_common.sh

exec sleep infinity
