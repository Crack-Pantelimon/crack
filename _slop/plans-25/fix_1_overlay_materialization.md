# Fix 1 — Stop patches from spuriously deleting tracked-but-gitignored files

**Segment 1 of 6. Implement first. Python-only, low risk.**

## What is broken

When a sandboxed chat finishes, the harness extracts the sandbox's changes into a `patch.diff`
and applies it to the host with `git apply`. For chat `1784739382358` that patch was 150 entries:
the 2 files the agent actually created **plus 148 spurious deletions** of tracked cache files under
`_data/3d_data_v2/data_cache/**/*.bytes`. Host `git apply` then fails **deterministically**:

```
error: cannot apply binary patch to '_data/.../008350de….bytes' without full index line
error: _data/.../008350de….bytes: patch does not apply
```

This failing patch is the *fuel* for the whole failure cascade (segment 2 is the engine that
loops on it). Fixing it here makes the vast majority of apply-failures disappear.

## Root cause (confirmed by evidence)

The sandbox is an overlay whose read-only lower is a **frozen git snapshot** of the repo's tracked
tree, created by `materialise_frozen_base()` in `sandbox.py`. That function runs `git init` in the
frozen base, which leaves an **empty git index**.

Patch extraction (`_produce_diff` in `patch.py`) then does, inside the sandbox:
1. `git add -A` → build the "end" tree from the working directory,
2. `git write-tree` → `end_tree`,
3. `git diff <base_tree> <end_tree>` → the patch.

`base_tree` is the host's real tree id and **does** contain the 148 `_data/**/*.bytes` files (they
are tracked on the host — force-added past a nested `.gitignore`). But because the sandbox index
started **empty**, step 1's `git add -A` sees those `.bytes` files as *ignored + untracked* (a
nested `.gitignore` ignores them, and nothing in the empty index says they're tracked), so it
**skips them**. They are therefore absent from `end_tree`, and `git diff base_tree end_tree` reports
all 148 as deletions.

Verified counts on the broken chat: `base_tree` references exactly 148 `.bytes`; the frozen base
physically contains all 148; the patch deletes all 148. It is purely the empty-index behavior.

## The fix

Seed the sandbox git **index** from `base_tree` with `git read-tree <base_tree>` **before** staging,
so `git add -A` computes a true delta (untouched tracked files — even ignored ones — stay in the
index at their base blob; only genuine changes are diffed). Do this in **both** `_produce_diff` and
`_produce_diff_sync` in `.pi/crack/server/src/crack_server/patch.py`.

### Prove the mechanism first (no code, ~30s) — run this to see the bug and the fix

```bash
docker exec crack-dev bash -lc '
d=$(mktemp -d); cd "$d"; git init -q; git config user.email t@t; git config user.name t
echo cache > data.bytes; printf "data.bytes\n" > .gitignore
git add -f data.bytes .gitignore; git commit -qm base
BASE=$(git write-tree)
# simulate the sandbox: fresh EMPTY index, identical working tree
rm -f .git/index
git add -A; END=$(git write-tree)
echo "--- BUG (empty index): expect a deletion of data.bytes ---"
git diff --stat $BASE $END
# THE FIX: seed the index from base first
git read-tree $BASE; git add -A; END2=$(git write-tree)
echo "--- FIXED (read-tree first): expect NO changes ---"
git diff --stat $BASE $END2 || echo "(empty diff = fixed)"
rm -rf "$d"'
```
You should see `data.bytes` deleted in the BUG block and an empty diff in the FIXED block.

### Code change

Open `.pi/crack/server/src/crack_server/patch.py`. Find `_produce_diff` (async) and
`_produce_diff_sync`. They currently look like:

```python
async def _produce_diff(sandbox_name, base_tree, patch_path, *, exclude=()):
    await _stage_for_patch(sandbox_name, exclude=exclude)
    end_tree = await _write_tree(sandbox_name)
    ...
```

Add a `git read-tree <base_tree>` **before** `_stage_for_patch`, in each function. Use the existing
in-sandbox git helpers (`_git_in_sandbox` for async, `_git_in_sandbox_sync` for sync — grep them in
the same file). Async version:

```python
async def _produce_diff(sandbox_name, base_tree, patch_path, *, exclude=()):
    # Seed the index from the frozen base tree so `git add -A` computes a true
    # delta. Without this the sandbox's git repo was `git init`'d with an empty
    # index, so `git add -A` skips tracked-but-gitignored files (e.g. _data/**/*.bytes)
    # and every diff spuriously "deletes" them — which host `git apply` cannot apply.
    rc, _, err = await _git_in_sandbox(sandbox_name, "read-tree", base_tree)
    if rc != 0:
        raise RuntimeError(f"git read-tree {base_tree[:12]} failed: {err}")
    await _stage_for_patch(sandbox_name, exclude=exclude)
    end_tree = await _write_tree(sandbox_name)
    ...
```

Do the exact same for `_produce_diff_sync` using `_git_in_sandbox_sync`. Change nothing else in
those functions.

> Note: `base_tree` is already proven resolvable in the sandbox here — the very next line,
> `git diff base_tree end_tree`, resolves it the same way (via the frozen base's git-object
> alternates). So `read-tree base_tree` will resolve identically.

## Build / restart

```bash
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh     # ./build.sh not needed (Python-only)
```

## Verify

### 1. Unit tests stay green
```bash
docker exec crack-dev bash -lc \
  'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
```
Also inspect `tests/test_patch.py`; if it has a diff-production test with a fake/real sandbox, add a
case asserting that an **untouched tracked file does not appear as a deletion** and that a newly
created file *does* appear. Follow the existing test style in that file (don't invent a new harness).

### 2. End-to-end: a fresh chat produces a clean, applyable patch
Create a chat, send one message that writes a single file, wait for it to finish, and confirm the
patch has **no `_data` deletions** and applied cleanly (chat goes `idle` with no `patch_apply`
exchange).

```bash
# create a chat, capture its id from the redirect
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "chat id: $CID"
# send one simple, deterministic instruction (plan mode off keeps it to ~1 hop)
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create a file at /workspace/hello_fix1.txt containing exactly the word: hi. Then stop.' \
  --data-urlencode 'plan=' >/dev/null
# wait until the chat is idle (poll chat.json phase; up to ~3 min for the model)
for i in $(seq 1 60); do
  P=$(docker exec crack-dev bash -lc "jq -r .phase /crack-harness-data/unscripted_chats/$CID/chat.json 2>/dev/null")
  echo "phase=$P"; [ "$P" = idle ] || [ "$P" = error ] && break; sleep 3
done
# INSPECT the extracted patch
docker exec crack-dev bash -lc "
  echo '--- deletions in patch (must be 0) ---';
  grep -c '^deleted file' /crack-harness-data/unscripted_chats/$CID/patch.diff 2>/dev/null || echo 'no patch file';
  echo '--- diff entries (should be ~1: just hello_fix1.txt) ---';
  grep '^diff --git' /crack-harness-data/unscripted_chats/$CID/patch.diff 2>/dev/null | head;
  echo '--- exchanges: there must be NO source=patch_apply ---';
  jq -r '.exchanges[]?.source' /crack-harness-data/unscripted_chats/$CID/chat.json"
```

**PASS criteria:**
- `deleted file` count in `patch.diff` is **0**.
- The only `diff --git` entry is `hello_fix1.txt` (plus at most unrelated files the agent genuinely
  touched — never `_data/*.bytes`).
- `chat.json` `exchanges[].source` contains **no** `patch_apply` entries.
- The chat ended in `phase: idle`, not `error`, and the server log shows no
  `patch apply … failed` WARNING for this chat:
  `docker logs crack-dev 2>&1 | grep "$CID" | grep -i "patch apply"` → empty.

### 3. Clean up the scratch chat
```bash
curl -s -X DELETE "http://localhost:9847/api/chats/$CID" >/dev/null
```

## Done when
All unit tests green, and a fresh single-file chat yields a patch with zero `_data` deletions that
applies to the host without any `patch_apply` follow-up exchange.
