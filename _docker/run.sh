#!/bin/bash

set -ex
export IMG_NAME=crack-dev:latest

if ! ( docker volume ls | grep crack-dev-root-dir ) ; then 
    docker volume create crack-dev-root-dir
fi

if ! ( docker volume ls | grep crack-dev-target-dir ) ; then 
    docker volume create crack-dev-target-dir
fi

docker rm -f crack-dev || true
# ./build.sh

docker run -d \
  --name crack-dev \
  -v "$(dirname $PWD):/workspace" \
  -v "crack-dev-root-dir:/root" \
  -v "crack-dev-target-dir:/workspace/target" \
  -p "127.0.0.1:9847:9847" \
  -p "127.0.0.1:21122:22" \
  -p "127.0.0.1:9930:9930" \
  -p "127.0.0.1:9931:9931" \
  -p "127.0.0.1:9932:9932" \
  --init \
  $IMG_NAME /bin/bash _docker/_cont_start.sh