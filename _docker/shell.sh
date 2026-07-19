#!/bin/bash
set -e
docker exec -it crack-dev /bin/bash -exc "tmux a || tmux new"