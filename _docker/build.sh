#!/bin/bash

set -ex
export IMG_NAME=crack-dev:latest

docker build -t $IMG_NAME -f Dockerfile .

