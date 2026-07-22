# Fix 5 — Drive pi over `--mode rpc` (authoritative completion, real abort)

**Segment 5 of 6. Implement after fix_1–4 are verified. HIGH RISK — new control plane. Build it
behind a feature flag so the existing `--mode json` path keeps working until fix_6 flips the default.**

## Why

Today pi is driven by `pi --mode json` as a **fire-and-forget** subprocess: the server writes pi's
stdout to a file, *tails* it, and **infers** "crashed" whenever the JSON event stream ends without a
terminal event. That inference is wrong often: it produces false "pi crashed mid-turn" classifications,
spawns a second pi on the same session (duplicate session files, replayed prompts), and the error
detail it surfaces is a useless startup warning instead of the real error.

`pi --mode rpc` replaces inference with an **authoritative** request/response + event protocol:
- send one `prompt` command (the user message goes over the wire exactly once — no replay),
- read events until **`agent_settled`** (definitive "fully done, nothing more will run"),
- send **`abort`** to stop cleanly,
- real errors arrive as structured events (`auto_retry_end` with `success:false` + `finalError`,
  `message_update` with an `error` delta, or a `response` with `success:false`).

This segment builds a working RPC hop runner behind `CRACK_PI_RPC=1`. **Segment 6** delegates LLM
retries to pi, surfaces the exact errors, adds reload-survival, makes RPC the default, and deletes the
old crash-inference machinery.

## Read the protocol first

The RPC protocol is documented inside the container. Read it before coding:
```bash
docker exec crack-dev bash -lc 'find / -path "*pi-coding-agent/docs/rpc.md" 2>/dev/null | head -1 | xargs sed -n "1,200p"'
```
Key facts (JSONL over stdin/stdout, **split on `\n` only**):
- Commands (stdin, one JSON per line): `{"id":"1","type":"prompt","message":"..."}`, `{"type":"abort"}`,
  `{"type":"get_state"}`, `{"type":"set_auto_retry","enabled":true}` (used in fix_6).
- Responses (stdout): `{"type":"response","command":"prompt","success":true,"id":"1"}`.
- Events (stdout, no `id`): `agent_start`, `turn_start`, `turn_end`, `message_start`, `message_end`,
  `message_update`, `tool_execution_*`, `auto_retry_start`, `auto_retry_end`, `agent_end`,
  **`agent_settled`**. `agent_end` may be followed by a retry/continuation; **only `agent_settled`
  means truly done.**
- The event **shapes match `--mode json`** (`turn_end`, `message_end`, etc.), so the existing turn
  accumulation code is reusable (see below).

## What to build

### 1. Add stdin support to the sandbox exec helper

`sandbox.py` `exec_in(...)` builds `podman exec`. Add an interactive variant (or a `stdin`/`interactive`
param) that passes `-i` and wires `stdin=asyncio.subprocess.PIPE`, `stdout=PIPE`. Example addition:
```python
async def exec_in(name, argv, *, env=None, cwd="/workspace", detached=False,
                  stdout=None, stderr=None, interactive=False, stdin=None):
    cmd = ["podman", "exec"]
    if detached: cmd.append("-d")
    if interactive: cmd.append("-i")
    ...
    kwargs = {}
    if stdin is not None: kwargs["stdin"] = stdin
    if stdout is not None: kwargs["stdout"] = stdout
    if stderr is not None: kwargs["stderr"] = stderr
    return await asyncio.create_subprocess_exec(*cmd, **kwargs)
```

### 2. New module `crack_server/pi_rpc.py`

Implement `arun_agent_hop_rpc(**kwargs)` with the **same signature and return contract** as
`pi_runner.arun_agent_hop` (see its docstring in `pi_proc.py`): same kwargs
(`log_prefix, model, session_id, sessions_dir, tools, message, start, sentinel, timeout_seconds,
persist_turn, hop, pid_file, stop_check, record_prompt, record_error, error_budget, env_extra,
waiting_check, append_system_prompt, swap_after_edit, todo_already, sandbox`), returns a stop reason
string (`"agent_end" | "time_cap" | "stopped" | "empty" | "swap"`).

**Reuse, don't reinvent, the turn accumulation:**
- `crack_server.transcript.apply_event_to_turn(event, current_turn)`, `turn_has_content`,
  `text_from_content` — same helpers the json path uses.
- The `_TurnAccumulator` class and the `_process_stream_line` per-line logic in `pi_proc.py` already
  turn `turn_end`/`message_end`/sentinel/time-cap/swap events into persisted turns. You may import
  `_TurnAccumulator` from `pi_proc` (or copy its small body) and reuse the same turn-boundary logic.
  The only difference: **completion is authoritative** — you stop on `agent_settled` instead of
  guessing from an incomplete stream.

**Transport skeleton** (sandbox case; also support the non-sandbox local case for tests):
```python
async def arun_agent_hop_rpc(*, sandbox, session_id, sessions_dir, model, tools, message,
                             persist_turn, stop_check=None, record_prompt=None, record_error=None,
                             timeout_seconds, hop=1, sentinel=None, waiting_check=None,
                             append_system_prompt=None, env_extra=None, pid_file=None,
                             swap_after_edit=False, todo_already=False, **_ignored) -> str:
    argv = _build_rpc_cmd(model, session_id, sessions_dir, tools, append_system_prompt)  # like _build_cmd but: --mode rpc, NO trailing message
    proc = await sandbox_mod.exec_in(sandbox, argv, env=..., cwd="/workspace",
                                     interactive=True, stdin=PIPE, stdout=PIPE, stderr=PIPE)
    # record the exact prompt once (trajectory), exactly like the json path
    if record_prompt: record_prompt({"kind": "user_prompt", "compiled": message, "hop": hop, "at": time.time()})
    # send the single prompt
    proc.stdin.write((json.dumps({"id": "p1", "type": "prompt", "message": message}) + "\n").encode())
    await proc.stdin.drain()
    acc = _TurnAccumulator(); reason = "agent_end"; persisted = 0; start = time.monotonic()
    async for line in _iter_jsonl(proc.stdout):   # split on b"\n" only; ignore blank lines
        ev = json.loads(line)
        t = ev.get("type")
        if t == "response":            # command ack; success:false => surface as error (fix_6)
            continue
        acc.apply(ev)
        if t == "turn_end" and turn_has_content(acc.current_turn):
            persist_turn(acc.current_turn, hop); persisted += 1; acc = _TurnAccumulator()
        # stop / time-cap: send abort, then keep reading until agent_settled
        if stop_check and stop_check():
            proc.stdin.write(b'{"type":"abort"}\n'); await proc.stdin.drain(); reason = "stopped"
        if (time.monotonic() - start) > timeout_seconds and reason not in ("stopped",):
            proc.stdin.write(b'{"type":"abort"}\n'); await proc.stdin.drain(); reason = "time_cap"
        if t == "agent_settled":
            break
    await _shutdown(proc)   # close stdin, wait with a grace, then kill the session if still alive
    if reason == "agent_end" and persisted == 0:
        reason = "empty"
    return reason
```
Notes:
- `_iter_jsonl` must split on `\n` **only** (do not use anything that also splits on U+2028/U+2029).
- Honor `waiting_check` for the time-cap exactly like the json path (credit server-side wait time)
  if easy; otherwise keep the plain wall-clock cap for now and note it.
- `swap_after_edit`: keep parity with the json path (end the hop with reason `"swap"` on the first
  edit/write turn after a todo exists). Reuse the same detection the json `_process_stream_line`
  uses. If this is hard to port in one pass, gate it: when `swap_after_edit` is set, fall back to the
  json path for that hop (see the flag in step 3) and leave a `TODO(fix6)`.
- Do **not** implement pi-side auto-retry here — that is fix_6. For now, a hard failure (process
  exits, or `auto_retry_end success:false`) should just end the hop; record it via `record_error`
  with the event's message and return a non-clean reason. Keep it simple; fix_6 hardens it.

### 3. Wire it behind a flag (non-destructive)

In `pi_runner`/`pi_proc.arun_agent_hop`, at the top, delegate to RPC when the env flag is set **and**
a sandbox is used:
```python
import os
async def arun_agent_hop(*, sandbox=None, **kwargs):
    if os.environ.get("CRACK_PI_RPC") and sandbox:
        from crack_server import pi_rpc
        return await pi_rpc.arun_agent_hop_rpc(sandbox=sandbox, **kwargs)
    ...  # existing json-mode implementation unchanged
```
Set `CRACK_PI_RPC=1` for crack-dev to test. The env is set in `_docker/run.sh` (add `-e CRACK_PI_RPC=1`
to the `docker run` for crack-dev) **or** exported before launching. Do NOT delete the json path in
this segment.

## Build / restart
```bash
# add `-e CRACK_PI_RPC=1` to the crack-dev `docker run` in _docker/run.sh, then:
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh
```
(No image rebuild needed for Python edits; `./build.sh` only if you change the Dockerfile.)

## Verify

### 1. Unit tests green
```bash
docker exec crack-dev bash -lc \
  'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
```
Add `tests/test_pi_rpc.py`: drive `arun_agent_hop_rpc` against a **fake** RPC process (a small script
that emits a scripted JSONL event sequence: `agent_start`, a `turn_end` with content, `agent_settled`)
and assert it persists one turn and returns `"agent_end"`. Add an abort test: when `stop_check`
returns true mid-stream, it writes an `abort` command and returns `"stopped"`. Follow existing test
patterns for faking subprocesses (grep `tests/` for how the json path is faked).

### 2. Live: a single-hop chat runs to completion over RPC
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create /workspace/hello_fix5.txt with the word hi. Then stop.' \
  --data-urlencode 'plan=' >/dev/null
# confirm the server used the RPC path and there is exactly ONE session file (no duplicates)
docker logs crack-dev 2>&1 | grep "$CID" | grep -iE "rpc|--mode rpc" | head
docker exec crack-dev bash -lc "ls -1 /crack-harness-data/unscripted_chats/$CID/sessions | wc -l"
docker exec crack-dev bash -lc "jq -r '.phase' /crack-harness-data/unscripted_chats/$CID/chat.json"
```
**PASS:** the chat reaches `phase: idle`, produces the file, and there is **exactly one** session
file (the json path could produce several). Clean up: `curl -s -X DELETE http://localhost:9847/api/chats/$CID`.

### 3. Live: STOP aborts cleanly over RPC
Send a longer task, `POST /stop` mid-run, and confirm the server sent an `abort` and the hop returned
`stopped` with no duplicate spawn (combine with fix_3's durable-stop checks).

## Done when
With `CRACK_PI_RPC=1`, a chat runs to completion over the RPC protocol producing a single session
file, STOP aborts cleanly, and the json path still works with the flag off. Retries/exact-errors/
reload-survival and making RPC the default are **fix_6**.
