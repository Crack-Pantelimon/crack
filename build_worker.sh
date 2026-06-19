#!/bin/bash
set -ex
# rm -rf packages/web_serviceworker_crackslave/target/wasmpack-crackslave || true
# time wasm-pack build \
#   --target no-modules \
#   --out-dir target/wasmpack-crackslave \
#   packages/web_serviceworker_crackslave \
#   --no-opt --no-typescript --profile dev --help

# rm -rf $OUT_DIR
cargo build --package web_worker --target wasm32-unknown-unknown
export WASM_FILE="target/wasm32-unknown-unknown/debug/web_worker.wasm"
export OUT_DIR="crack_demo/web_frontend/assets/pkg_web_serviceworker"
wasm-bindgen \
   --keep-debug --debug --keep-lld-exports \
   --target no-modules  --no-typescript \
   --out-dir "$OUT_DIR" \
   "$WASM_FILE"
md5sum "$WASM_FILE" | cut -f1 -d' ' > "$OUT_DIR/md5.txt"
echo "//#region: crack"                                                                      >> $OUT_DIR/web_worker.js
echo "let __wasm_script_md5 =   '$(cat $OUT_DIR/md5.txt)';"  >> $OUT_DIR/web_worker.js


# sed -i "s/let __wasm_worker_md5 = .*/let __wasm_worker_md5 = \"$(cat $OUT_DIR/md5.txt)\";  /" crack_demo/web_frontend/assets/scripts/worker.js


