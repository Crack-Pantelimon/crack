#!/bin/bash
# Build and push the crack-dev base image (manual — not part of compose).
# The derived crack-dev image (Dockerfile) layers on top of this.
set -ex
cd "$(dirname "$0")"
docker build -f Dockerfile.base -t docker.io/johnnysmitherson/crack-dev:base .
docker push docker.io/johnnysmitherson/crack-dev:base
