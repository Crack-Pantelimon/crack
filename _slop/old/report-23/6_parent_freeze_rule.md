# Plan 6 report — Parent freeze: implicit wait_join before any destructive tool

> The Plan 6 code shipped in the parts 1–6 commit (`6386b3c`); its live test was
> "cut mid-way". This report is written retroactively by the Plan 7 agent, who found
> and fixed the bug that actually blocked the Plan 6 live test.

## What shipped (parts 1–6)

- **Extension hook** (`.pi/extensions/crack/index.ts`): when `canSpawn` (depth <
  MAX_DEPTH), a `tool_call` handler runs before every destructive tool. `FREE_TOOLS`
  (`read/grep/find/ls/todo/wait_join/ask_user/analyze_image`) and every `spawn_*` are
  exempt; everything else (`bash`, `edit`, `write`, all MCP/custom) is destructive.
  On a destructive call it probes `hasRunningChildren()` and, if any, calls
  `waitForAllChildren()` (loops `executeWaitJoin` until the child count hits 0).
- **Server endpoint**: `GET /api/chats/{chat}/sub_agents/active_count?parent_kind=&parent_id=`
  → `{"active": N}` via `runner.active_child_count` (non-terminal children; works for
  both `chat` and `run` parents). One stat-cheap call per destructive tool.

Spawn parallelism and the `MAX_PARALLEL_SUBAGENTS=3` slot limit are untouched (`spawn_*`
is exempt from the freeze; only the existing slot back-pressure can make a spawn wait).

## Why the live test "cut mid-way" — and the fix

The freeze test needs a sandboxed chat to actually **spawn** sub-agents. It couldn't:
`pi_proc._spawn_sandbox_pi` built the sandbox `podman exec` env with
`CRACK_PI_HOST=os.environ.get("CRACK_PI_HOST", "crack-dev")`, which inside crack-dev is
`0.0.0.0` (uvicorn's *bind* address from `_cont_start.sh`). That leaked into the sandbox
and overrode the container's correct `crack-dev`, so the extension built
`BASE=http://0.0.0.0:9847` and **every** `spawn_*`/`wait_join`/`ask_user` from a
sandboxed chat failed with `ECONNREFUSED 0.0.0.0:9847`. Fixed in Plan 7 by pinning
`CRACK_PI_HOST="crack-dev"` in `_spawn_sandbox_pi`. After the fix, a sandboxed
nemotron chat spawned coder sub-agents successfully (observed live).

## Robustness change (Plan 7)

`hasRunningChildren()` previously **threw** on any probe failure, hard-failing the
underlying destructive tool whenever crack-dev was briefly unreachable (e.g. a uvicorn
reload). It now retries 3× then **fails open** (treats as "no children"). This is safe:
with the Plan 7 git-replay overlays, a child no longer mounts the parent's live tree, so
a missed freeze during a reload window cannot corrupt a child's lower.

## Interaction with the Plan 7 patch guard

When the implicit wait returns, the finished children's patches have been applied into
the parent overlay **in dispatch order** by `drain_parent_patches` (Plan 7), so the
parent's subsequent `edit`/`bash` sees the merged result. Conflicts are handed to the
managing agent (Plan 4 wording).

## Verification status

- **Endpoint**: unit-tested (`test_active_child_count_endpoint`) — 0 → 1 after spawn →
  0 after finish.
- **Freeze ordering** (write blocks until children terminal), **reads don't freeze**,
  **parallel spawn doesn't wait**, **slot limit intact**, **leaf agent unaffected**:
  the *mechanism* (endpoint + hook + fail-open) is in place and unit-covered for the
  count; the full five-transcript live matrix from the plan was **not** re-run to
  completion here because nemotron-super was too flaky at multi-step tool ordering
  during this session (it would spawn without first editing, etc.). The spawn/probe
  plumbing itself is now confirmed working end-to-end (sub-agents spawn from sandboxes;
  the `active_count` probe is reachable — `curl` from a sandbox returns 200).

## Deliberate behavior (for reviewers)

`bash` is destructive even for read-only commands (`git status`, `cat`) — intentional,
since intent can't be parsed. Agents should use `read/grep/ls/find` for inspection while
children run. `wait_join`, `spawn_*`, and `ask_user` are in the free set to avoid a
self-deadlock while "frozen".
