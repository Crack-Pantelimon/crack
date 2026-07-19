# Fix: wasm build stuck at root because the browser runs a stale cached worker wasm

## Context

The web build stopped refining past the root LOD level after the BVH/LOD changes, while
the native build works. **This is not a bug in game_logic** — the code on disk is correct
and identical for both targets:

- The warnings the browser prints (`Skipping degenerate occluder tile`,
  `Occluder world (lock-step) built ... (63 leaves)`) **exist nowhere in current source**
  (`rg` → 0 matches); they are from a previous session's code.
- The on-disk worker artifact
  `demo_resolution_selector_web_bevy/public/pkg_web_serviceworker/web_worker_bg.wasm`
  **is current**: it contains the new `"Occluder world sync"` string, none of the old
  strings, and its md5 matches the freshly built `web_worker.wasm`.
- The artifact was rebuilt at 18:02; the browser log with the old strings is from 19:34.
  So the browser executed a **cached pre-18:02 wasm**.

Root cause: the worker loads its wasm + glue from **stable, non-versioned URLs**
(`/pkg_web_serviceworker/web_worker_bg.wasm` and `web_worker.js`). A rebuild does not
change the URL, so the browser HTTP cache keeps serving the old 14 MB `.wasm`. `build_worker.sh`
already computes `md5.txt` but nothing wires it into a fetch URL. (The `pwa_sw.js` service
worker is commented out in `index.html`, so it is not involved.)

Decision (user): the cache-bust is **owned by `build_worker.sh`** (the single generation
point that knows the md5). The hand-written loader keeps a version placeholder that the
build fills in.

Outcome: after any worker rebuild, the loader requests a new URL, so the browser can never
serve stale worker code again; and the artifact itself reliably rebuilds when game_logic changes.

## Changes

### 1. Templatize the loader URLs with a version placeholder (tracked source)
Files (both copies, git-tracked, hand-written):
- `crack_demo/demo_resolution_selector_web_bevy/public/scripts/v2/crack2-dedicated-worker.js`
- `crack_demo/web_frontend/assets/scripts/v2/crack2-dedicated-worker.js`

Append `?v=__WASM_MD5__` to both worker-asset URLs in each file:
- demo line 1:  `importScripts("/pkg_web_serviceworker/web_worker.js?v=__WASM_MD5__");`
- demo line 16: `await wasm_bindgen("/pkg_web_serviceworker/web_worker_bg.wasm?v=__WASM_MD5__");`
- web_frontend: same, with the `/assets/pkg_web_serviceworker/` prefix.

`__WASM_MD5__` is the committed placeholder. Both the glue (`web_worker.js`) and the wasm
are versioned with the same token so a stale glue/wasm ABI mismatch cannot occur.

### 2. `build_worker.sh` fills the placeholder each build (generation owns it)
File: `build_worker.sh`. Capture the md5 into a variable (it already computes it for
`md5.txt`), and after the `cp -r "$OUT_DIR" "$OUT_DIR2"` step, run an **idempotent** sed
over both served loaders:

```bash
MD5="$(md5sum "$WASM_FILE" | cut -f1 -d' ')"
echo "$MD5" > "$OUT_DIR/md5.txt"
# ... existing __wasm_script_md5 append + cp -r ...
for LOADER in \
  crack_demo/demo_resolution_selector_web_bevy/public/scripts/v2/crack2-dedicated-worker.js \
  crack_demo/web_frontend/assets/scripts/v2/crack2-dedicated-worker.js ; do
  sed -i -E "s#(web_worker(_bg\.wasm|\.js))\?v=[A-Za-z0-9_]+#\1?v=${MD5}#g" "$LOADER"
done
```

The regex matches both the initial `?v=__WASM_MD5__` and any prior `?v=<oldmd5>`, so it is
re-runnable (idempotent) — every build rewrites the token to the current md5. `deploy.sh`
already runs `build_worker.sh` before `rsync`, so deployed loaders carry the correct hash too.

Tradeoff (call out to user): after a build the two tracked loader files show the real md5
in the working tree — an expected build-artifact diff. Commit them in their `__WASM_MD5__`
placeholder form; the filled value is local/deploy-time state, not something to commit.

### 3. Close the rebuild watch gap (prevention)
File: `start_builder.sh`. The `cargo watch` invocation watches `packages/`, `src/`,
`crack_demo/web_worker` — but **not `crack_demo/game_logic`**, even though `web_worker`
depends on `game_logic` (`crack_demo/web_worker/Cargo.toml:15`). Add
`--watch crack_demo/game_logic \` so edits to lod.rs/visibility.rs rebuild the worker wasm
(and thus refresh the md5). Without this, the served wasm can silently lag source even with
the cache-bust in place.

## Non-goals
- Native `compute_lod_changes` currently logs 73–90 ms during initial load
  (`Occluder world sync: +79 occluders in 71 ms`) — a separate, pre-existing cold-start
  cost of the turn-2 occluder-sync design, not this bug. Fresh wasm will load past root
  (the fail-open ray budget guarantees progress) but cold start will be slower than native.
  Only address if the user asks after confirming the cache fix.

## Verification
1. `unset ARGV0; ./build_worker.sh` — rebuild + fill loaders. Confirm both
   `crack2-dedicated-worker.js` now contain `?v=<the-md5>` matching
   `public/pkg_web_serviceworker/md5.txt` (`grep -o 'v=[a-f0-9]*'`).
2. `unset ARGV0; RUST_LOG=info ./start_game_web.sh` (or `trunk serve`). In DevTools do one
   **hard refresh** (to drop the currently-cached old loader + wasm); thereafter normal
   reloads suffice.
3. In the worker console verify it fetches `web_worker_bg.wasm?v=<md5>` and prints
   `Occluder world sync: +N ...` (new string) — never `lock-step) built` /
   `Skipping degenerate occluder tile`. The map refines past root.
4. Edit-loop check: change a `tracing` string in `game_logic/src/lod.rs`; confirm
   `start_builder.sh` rebuilds the worker, the md5/`?v=` changes, and a normal reload shows
   the new string in the browser worker log (no manual cache clear).
