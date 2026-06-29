#!/bin/bash
set -ex

cd _data
docker-compose up -d
docker restart crack_nginx_data