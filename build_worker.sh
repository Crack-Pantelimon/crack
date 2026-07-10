#!/bin/bash
set -ex

cargo build --package web_worker --target wasm32-unknown-unknown
export WASM_FILE="target/wasm32-unknown-unknown/debug/web_worker.wasm"
export OUT_DIR="crack_demo/demo_resolution_selector_web_bevy/public/pkg_web_serviceworker"
wasm-bindgen \
   --keep-debug --debug --keep-lld-exports \
   --target no-modules  --no-typescript \
   --out-dir "$OUT_DIR" \
   "$WASM_FILE"
MD5="$(md5sum "$WASM_FILE" | cut -f1 -d' ')"
echo "$MD5" > "$OUT_DIR/md5.txt"
echo "//#region: crack"                                                                      >> $OUT_DIR/web_worker.js
echo "let __wasm_script_md5 =   '$(cat $OUT_DIR/md5.txt)';"  >> $OUT_DIR/web_worker.js


export LOADER="crack_demo/demo_resolution_selector_web_bevy/public/scripts/v2/crack2-dedicated-worker.js"
sed -i -E "s#(web_worker(_bg\.wasm|\.js))\?v=[A-Za-z0-9_]+#\1?v=${MD5}#g" "$LOADER"
