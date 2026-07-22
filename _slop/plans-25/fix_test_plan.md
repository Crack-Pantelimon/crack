# Live test plan — verify fixes 1–6 by driving real chats

You are a test driver. The six code fixes (`fix_1`…`fix_6`) are already implemented and the unit
suite is green (178 passed). Your job is to **drive real chats through the app** and confirm each fix
behaves correctly against a live pi model, then **write your results to
`_slop/plans-25/fix_test_report.md`** using the template at the bottom.

Do **not** edit any source code. If a test fails, record the failure verbatim (commands + output) —
do not try to fix it.

---

## 0. Environment & primitives

- App UI: `http://localhost:9847` . A chat page: `http://localhost:9847/chats/<id>` .
- All tooling (`jq`, `git`, `podman`) is **inside** the `crack-dev` container: prefix with
  `docker exec crack-dev bash -lc '…'`. `curl` works from the host against `localhost:9847`.
- Chat state on disk: `/crack-harness-data/unscripted_chats/<id>/chat.json` (inside crack-dev).
- The server does NOT auto-reload. It is already running; do not restart it unless a test step says
  so.

### Create a chat and get its id
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "chat id: $CID"
```

### Send the FIRST message in single-hop (plan-mode OFF) mode — use this for deterministic tests
`config=1` with **no** `plan` field turns plan mode off so the run is ~1 hop (no planner/implementer
swap). Always send `config=1` on the *first* message of a chat you want to be deterministic:
```bash
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=YOUR INSTRUCTION HERE' \
  --data-urlencode 'config=1' >/dev/null
```
Follow-up messages in the same chat need only `msg=…`.

### Wait until a chat settles
```bash
wait_settled() {  # $1 = chat id, optional $2 = max polls (default 60 * 3s = 3min)
  for i in $(seq 1 "${2:-60}"); do
    P=$(docker exec crack-dev bash -lc "jq -r .phase /crack-harness-data/unscripted_chats/$1/chat.json 2>/dev/null")
    echo "  phase=$P"
    [ "$P" = idle ] || [ "$P" = error ] && break
    sleep 3
  done
}
```

### Inspect state
```bash
docker exec crack-dev bash -lc "jq -r '{phase,stop_requested,error,error_detail}' /crack-harness-data/unscripted_chats/$CID/chat.json"
```

### Delete a scratch chat (cleanup)
```bash
curl -s -X DELETE "http://localhost:9847/api/chats/$CID" >/dev/null
```

Run the tests **in order**. Each test says what to create, how to drive it, and the PASS criteria.

---

## Test 1 (fix_1) — patches don't spuriously delete `_data/*.bytes`

**Create & drive:**
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "fix1 chat: $CID"
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create a file at /workspace/hello_fix1.txt containing exactly the word: hi. Then stop.' \
  --data-urlencode 'config=1' >/dev/null
wait_settled "$CID"
```

**Verify:**
```bash
docker exec crack-dev bash -lc "
  echo '--- deletions in patch (must be 0) ---';
  grep -c '^deleted file' /crack-harness-data/unscripted_chats/$CID/patch.diff 2>/dev/null || echo 'no patch file';
  echo '--- diff entries (expect ~1: hello_fix1.txt) ---';
  grep '^diff --git' /crack-harness-data/unscripted_chats/$CID/patch.diff 2>/dev/null | head;
  echo '--- exchange sources (must contain NO patch_apply) ---';
  jq -r '.exchanges[]?.source' /crack-harness-data/unscripted_chats/$CID/chat.json;
  echo '--- final phase ---';
  jq -r .phase /crack-harness-data/unscripted_chats/$CID/chat.json"
docker logs crack-dev 2>&1 | grep "$CID" | grep -i "patch apply" || echo "(no patch-apply warnings — good)"
```

**PASS:** `deleted file` count is **0**; no `_data/*.bytes` in the diff; **no** `patch_apply` in
`exchanges[].source`; final `phase` is `idle` (not `error`); no "patch apply … failed" warning.
Record the id, the deletion count, and the diff-entry list. Then delete the chat.

---

## Test 2 (fix_2) — a host apply-failure records ONE error and never loops

With fix_1 in place real apply-failures are rare, so **force** one. The cleanest live way to make a
patch fail on the host is the **conflicting-host-file** method: pre-create the target file on the host
with different content, then ask the agent to create that same file — the resulting create-file patch
cannot apply cleanly on top of the pre-existing content.

```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "fix2 chat: $CID"
# Pre-create a host file with DIFFERENT content so the agent's create-file patch cannot apply cleanly.
docker exec crack-dev bash -lc 'printf "PREEXISTING\n" > /workspace/hello_fix2.txt'
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create a NEW file at /workspace/hello_fix2.txt containing exactly: hi. Then stop.' \
  --data-urlencode 'config=1' >/dev/null
wait_settled "$CID"
```

**Verify** (watch for ~30s that nothing keeps re-spawning):
```bash
docker exec crack-dev bash -lc "jq -r '{phase,error,error_detail}' /crack-harness-data/unscripted_chats/$CID/chat.json"
echo '--- patch_apply exchanges (must be 0) ---'
docker exec crack-dev bash -lc "jq -r '[.exchanges[]?.source] | map(select(.==\"patch_apply\")) | length' /crack-harness-data/unscripted_chats/$CID/chat.json"
echo '--- pi spawn count now, then again in 30s (must NOT grow) ---'
docker logs crack-dev 2>&1 | grep "$CID" | grep -c "podman exec -i"
sleep 30
docker logs crack-dev 2>&1 | grep "$CID" | grep -c "podman exec -i"
```

**PASS:** if the apply failed, `phase` is `idle` with a non-empty `error` about "could not be applied
to the host repo (git apply failed)"; the `patch_apply`-exchange count is **0**; the pi-spawn count
does **not** grow over the 30s window. (If the agent happened to produce an applyable patch anyway —
possible — note that the apply *succeeded* and this live test is inconclusive; the unit test
`test_finalize_chat_sandbox_apply_failure_does_not_enqueue` already proves the no-loop behavior.)
Cleanup: delete the chat and `docker exec crack-dev bash -lc 'rm -f /workspace/hello_fix2.txt'`.

---

## Test 3 (fix_3) — STOP is durable and only a human message resumes

**Create & drive a longer task, then STOP mid-run:**
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "fix3 chat: $CID"
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=List every file under /workspace/.pi recursively, then summarize in detail what each top-level dir does.' \
  --data-urlencode 'config=1' >/dev/null
sleep 8
curl -s -X POST "http://localhost:9847/api/chats/$CID/stop" >/dev/null
```

**Verify STOP sticks (watch ~30s):**
```bash
for i in $(seq 1 10); do
  docker exec crack-dev bash -lc "jq -r '\"stop=\"+(.stop_requested|tostring)+\" phase=\"+.phase' /crack-harness-data/unscripted_chats/$CID/chat.json"
  sleep 3
done
echo "--- pi spawns in the last 40s (should be 0 growth after stop) ---"
docker logs --since 40s crack-dev 2>&1 | grep "$CID" | grep -c "podman exec -i"
```
**PASS (part A):** `stop_requested` stays `true` and `phase` stays `idle`; no new `podman exec -i`
spawns appear after the stop.

**Verify a human message resumes it:**
```bash
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Just reply the single word RESUMED and stop.' >/dev/null
sleep 8
docker exec crack-dev bash -lc "jq -r '\"stop=\"+(.stop_requested|tostring)+\" phase=\"+.phase' /crack-harness-data/unscripted_chats/$CID/chat.json"
```
**PASS (part B):** after the human message `stop_requested` is `false` and the chat runs again
(`phase` goes `chatting` then `idle`). Cleanup: delete the chat.

---

## Test 4 (fix_4) — error rows render interleaved by time, not dumped at the bottom

Reuse a chat that has BOTH turns and at least one error row. The forced-failure chat from **Test 2**
(if it errored) or the **Test 6** overloaded-error chat are good candidates. If none has an error
row, skip and note "no error-bearing chat available".

**Verify the rendered order:**
```bash
CID=<a chat id that has turns AND error rows>
curl -s "http://localhost:9847/chats/$CID/status" | grep -oE 'traj-(error|turn|user_prompt|annotation)' | head -60
```
**PASS:** `traj-error` markers appear **between** `traj-turn`/`traj-user_prompt` markers in time
order — NOT all clustered after the final turn. Paste the marker sequence into the report.

---

## Test 5 (fix_5) — a chat runs over RPC and produces exactly one session file; STOP aborts cleanly

**Create & drive:**
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "fix5 chat: $CID"
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create /workspace/hello_fix5.txt with the word hi. Then stop.' \
  --data-urlencode 'config=1' >/dev/null
wait_settled "$CID"
```

**Verify RPC path + single session file:**
```bash
echo '--- server used --mode rpc for this chat ---'
docker logs crack-dev 2>&1 | grep "$CID" | grep -iE "podman exec -i|--mode rpc" | head
echo '--- session file count (must be exactly 1) ---'
docker exec crack-dev bash -lc "ls -1 /crack-harness-data/unscripted_chats/$CID/sessions 2>/dev/null | wc -l"
echo '--- final phase (idle) and the file exists ---'
docker exec crack-dev bash -lc "jq -r .phase /crack-harness-data/unscripted_chats/$CID/chat.json; ls -l /workspace/hello_fix5.txt 2>/dev/null || echo 'file missing'"
```
**PASS:** the log shows `podman exec -i … pi --mode rpc …`; there is **exactly one** session file;
`phase` is `idle`; `hello_fix5.txt` exists. Cleanup: delete the chat and remove the file.

**RPC STOP abort:** repeat the Test 3 STOP sequence on a fresh RPC chat and confirm the log shows an
`abort` was sent and the hop ended as `stopped` with **no duplicate spawn** for the same session id:
```bash
docker logs crack-dev 2>&1 | grep "$CID" | grep -iE "abort|stopped" | head
```
**PASS:** an abort/stopped is logged; only one pi process was spawned for the exchange.

---

## Test 6 (fix_6) — pi owns retries and the EXACT provider error reaches the banner

The goal is to see a real upstream error surface as the chat banner detail (not a generic
"pi crashed mid-turn" / "No project session found"). Upstream failures are intermittent, so this test
is **opportunistic**: drive a normal chat and, if pi reports a genuine provider failure, confirm the
banner carries the exact text.

**Drive:**
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
echo "fix6 chat: $CID"
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Say hello, then stop.' \
  --data-urlencode 'config=1' >/dev/null
wait_settled "$CID"
docker exec crack-dev bash -lc "jq -r '{phase,error,error_detail}' /crack-harness-data/unscripted_chats/$CID/chat.json"
```

**Interpretation / PASS:**
- If `phase` is `idle` with **no** error → pi succeeded (retries, if any, were handled inside pi).
  Confirm the absence of loops: `docker logs crack-dev 2>&1 | grep "$CID" | grep -ci "auto-retry"`
  may show pi auto-retry log lines; that is expected and fine. Record "succeeded, pi-owned retries".
- If `phase` is `error` → the `error`/`error_detail` MUST be a concrete provider/model error (e.g.
  contains an HTTP status like `429`/`529`, `overloaded`, `rate limit`, or a real model message). It
  MUST NOT be the old opaque `pi crashed mid-turn` or `No project session found`. Paste the exact
  `error`/`error_detail`.
- **Grep confirmation** (must find NO occurrences of the old opaque strings for this chat):
  ```bash
  docker logs crack-dev 2>&1 | grep "$CID" | grep -iE "pi crashed mid-turn|No project session found" || echo "(none — good)"
  ```

**Retry-from-error button (only if the chat errored):** the chat banner has a retry control. Trigger
it (re-send any human message via `POST /api/chats/$CID/messages msg=…`) and confirm the banner
clears and the chat runs again — i.e. `error` becomes empty and `phase` returns to `idle`.
Cleanup: delete the chat.

---

## Report

Write your findings to **`_slop/plans-25/fix_test_report.md`** using this template. For each test give
a verdict (PASS / FAIL / INCONCLUSIVE), the chat id(s) used, and the **actual command output** you
relied on. Keep raw output — do not paraphrase error text.

```markdown
# Live test report — fixes 1–6

Date/time: <…>
Server: http://localhost:9847  (running, not restarted)

## Test 1 — fix_1 no spurious _data deletions
Verdict: PASS/FAIL/INCONCLUSIVE
Chat id: <…>
Deletion count: <…>   Diff entries: <…>   patch_apply exchanges: <…>   Final phase: <…>
Evidence:
<paste>

## Test 2 — fix_2 apply-failure records one error, no loop
Verdict: …
Chat id: <…>
Apply failed? <yes/no>   patch_apply exchanges: <…>   pi-spawn count t0/t30: <…>/<…>
Evidence:
<paste>

## Test 3 — fix_3 durable STOP
Verdict (A: stop sticks): …   Verdict (B: human resumes): …
Chat id: <…>
stop_requested after stop (samples): <…>   spawns after stop: <…>
stop_requested/phase after human msg: <…>
Evidence:
<paste>

## Test 4 — fix_4 error interleaving
Verdict: …
Chat id: <…>
traj marker sequence:
<paste>

## Test 5 — fix_5 RPC single session file + clean abort
Verdict (session/completion): …   Verdict (abort): …
Chat id(s): <…>
Session file count: <…>   Used --mode rpc? <yes/no>   File created? <yes/no>
Evidence:
<paste>

## Test 6 — fix_6 exact error / pi-owned retries
Verdict: …
Chat id: <…>
Outcome: <succeeded | errored>
If errored, exact error / error_detail:
<paste>
Old opaque strings present? <no/yes — paste>

## Summary
- Passed: <…>
- Failed: <…>
- Inconclusive (with reason): <…>
- Any anomalies / notes for the lead: <…>
```

When done, ensure `_slop/plans-25/fix_test_report.md` exists and every test has a verdict. Delete all
scratch chats you created and remove any `/workspace/hello_fix*.txt` files.
