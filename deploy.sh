#!/bin/bash
set -ex

(
    cd crack_demo/demo_resolution_selector_web_bevy
    trunk build --release true
)

rsync -av --exclude 'target' --exclude '.git' ./ dj-vaslui:crack/

ssh dj-vaslui "cd crack && cd _data && docker compose up -d"

