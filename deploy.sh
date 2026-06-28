#!/bin/bash
set -ex

(
    cd crack_demo/demo_resolution_selector_web_bevy
    trunk build --release true
)

rsync -av --exclude 'target' --exclude '.git' --exclude ".venv" --exclude _data/3d_data_v2/data_cache/ --exclude '*.bytes' ./ dj-vaslui:crack/

ssh dj-vaslui "cd crack && cd _data && docker compose up -d && docker restart crack_nginx_data"

