#!/bin/bash
set -ex
# rm -rf packages/web_serviceworker_crackslave/target/wasmpack-crackslave || true
# time wasm-pack build \
#   --target no-modules \
#   --out-dir target/wasmpack-crackslave \
#   packages/web_serviceworker_crackslave \
#   --no-opt --no-typescript --profile dev --help

# rm -rf crack_demo2/assets/pkg_web_serviceworker
cargo build --package web_serviceworker_crackslave --target wasm32-unknown-unknown
wasm-bindgen \
   --keep-debug --debug \
   --target no-modules  --no-typescript \
   --out-dir crack_demo2/assets/pkg_web_serviceworker \
   ./target/wasm32-unknown-unknown/debug/web_serviceworker_crackslave.wasm
md5sum ./target/wasm32-unknown-unknown/debug/web_serviceworker_crackslave.wasm | cut -f1 -d' ' > crack_demo2/assets/pkg_web_serviceworker/md5.txt
echo "//#region: crack"                                                                      >> crack_demo2/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js
echo "let __wasm_script_md5 =   '$(cat crack_demo2/assets/pkg_web_serviceworker/md5.txt)';"  >> crack_demo2/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js


sed -i "s/let __wasm_worker_md5 = .*/let __wasm_worker_md5 = \"$(cat crack_demo2/assets/pkg_web_serviceworker/md5.txt)\";  /" crack_demo2/assets/scripts/worker.js


