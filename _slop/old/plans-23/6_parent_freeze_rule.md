# Plan 6 — Parent freeze: implicit wait_join before any destructive tool

> Read `0_overview.md`; requires Plans 1–3 (sandboxes exist) and benefits from Plan 4
> (auto-apply). This enforces the second half of "overlay lower must be stable": a parent
> must not mutate its tree while a child sandbox has that tree as its overlay **lower**.

## The rule (LOCKED)

While an agent (chat OR sub-agent) has **≥1 running child sub-agent**:
- Any **destructive** tool call — `bash`, `edit`, `write`, or **any MCP / custom tool** —
  first **implicitly `wait_join`s** (blocks until all running children finish), then proceeds.
- **Free** tools run immediately, never blocking: `read`, `grep`, `ls`, `find`, `todo`,
  `wait_join`, `ask_user`, `analyze_image`, and every `spawn_*`.
- **Spawning stays free:** `spawn_*` never triggers the implicit wait, so an agent can fire
  several sub-agents back-to-back for parallelism. Spawn's existing slot back-pressure
  (`MAX_PARALLEL_SUBAGENTS = 3`, `slot_pending`) is unchanged — that's the only thing that can
  make a spawn wait, and only at the limit.

Rationale: children read the parent's tree as a stable lower. The parent editing mid-flight
corrupts that lower. Freezing the parent's *writes* (not its reads) until children finish
guarantees a stable lower without serializing the whole system.

## Mechanism: the `tool_call` extension hook

pi fires `tool_call` **before a tool executes** and the handler **can block** (async). Add to
`.pi/extensions/crack/index.ts`, inside `export default function crack(pi)`:

```ts
const FREE_TOOLS = new Set([
  "read", "grep", "find", "ls", "todo", "wait_join", "ask_user", "analyze_image",
]);
function isDestructive(toolName: string): boolean {
  if (FREE_TOOLS.has(toolName)) return false;
  if (toolName.startsWith("spawn_")) return false;  // spawning must stay free
  return true;  // bash, edit, write, and ALL mcp/custom tools
}

pi.on("tool_call", async (event, _ctx) => {
  if (!canSpawn) return;                     // leaf agents have no children
  if (!isDestructive(event.toolName)) return;
  if (!(await hasRunningChildren())) return; // fast path: nothing to wait on
  // Block until every running child finishes; results are inboxed to the parent
  // and (Plan 4) auto-applied, so after this returns the parent may safely mutate.
  await executeWaitJoin({});                 // omit target => all outstanding
});
```

`hasRunningChildren()` must be **cheap** (it runs before every destructive call). Add a
lightweight server probe — reuse the wait endpoint with `block_seconds: 0` and read
`pending.length`, or add `GET /api/chats/{chat}/sub_agents/active_count?parent_kind=&parent_id=`
that returns the non-terminal child count (`sub_agents/runner.py:active_child_count` already
computes exactly this — expose it). Prefer the dedicated endpoint; it's one stat-cheap call.

Notes:
- The hook fires inside the **parent's** pi (in the parent's sandbox). `executeWaitJoin`
  already talks to crack-server over `BASE` (now `crack-dev:9847` via `CRACK_PI_HOST`, Plan 2).
- Do not mutate `event.input`; just `await` to delay execution. When the handler returns, pi
  runs the tool normally.
- If `wait_join` returns because the budget elapsed but children still run, **loop**: re-check
  `hasRunningChildren()` and wait again, so the invariant truly holds before the write. (Cap
  total wait generously; a stuck child should surface via the normal orphan watchdog.)

## Server: expose the active-child count

In `routes_sub_agents.py`, add a tiny GET route calling
`runner.active_child_count(chat_id, parent_kind, parent_id)`. This function already exists and
counts non-terminal children for both `chat` and `run` parents. Return `{"active": N}`.

## Interaction with Plan 4 (auto-apply)

When the implicit wait returns, each finished child's patch has already been auto-applied into
the parent overlay (Plan 4), so the parent's subsequent `edit`/`bash` sees the children's
work. If a child patch **conflicted**, the parent got the conflict message + patch path — the
implicit wait still returns (children are terminal), and the parent proceeds to resolve as
instructed. No special handling here.

## Verification (all via nemotron sample chats)

You need a persona to spawn. Use an existing one (`docker exec crack-dev bash -exc 'ls .pi/crack/sub_agents'`).

1. **Freeze on edit:** chat task: "Spawn two `<persona>` sub-agents, each to
   `sleep`-equivalent busy-work for ~30s (e.g. 'read three files and summarize'). Immediately
   after spawning both, create `/workspace/PARENT_EDIT.txt` with the word DONE, then stop."
   Watch the trajectory: the `write`/`bash` for `PARENT_EDIT.txt` must **not** execute until
   **both** sub-agent runs reach a terminal phase. Verify by timestamps in the persisted
   turns (the write's `elapsed`/`at` is after both children's `finished_at`).
2. **Reads don't freeze:** same setup but the parent does a `read`/`grep` right after
   spawning — it must return immediately (before children finish).
3. **Parallel spawn doesn't wait:** parent spawns 2 (below the limit of 3) back-to-back — the
   second spawn returns without an implicit wait (no "waited for a free slot" prefix, since
   under the limit).
4. **Limit back-pressure intact:** parent spawns 4; the 4th shows the existing
   `⏳ waited for a free slot` behavior (unchanged from before this plan).
5. **Leaf agent unaffected:** a sub-agent (depth 1, `canSpawn=false`) never installs the hook
   / never blocks — verify a sub-agent can `edit`/`bash` freely.

Confirm the `active_count` endpoint: `docker exec crack-dev bash -exc 'curl -s "http://127.0.0.1:9847/api/chats/<id>/sub_agents/active_count?parent_kind=chat&parent_id=<id>"'`.

## Gotchas

- The hook must be **installed only when `canSpawn`** (depth < MAX_DEPTH) — leaf agents have
  no children and must never pay the probe cost or risk a deadlock.
- Avoid a self-deadlock: `wait_join`, `spawn_*`, `ask_user` must be in the free set so the
  agent can still manage children while "frozen".
- The probe runs before *every* destructive call — keep it to one fast HTTP GET; do not run
  the full 10s-grace `executeWaitJoin` just to count. Only call `executeWaitJoin` when the
  count is > 0.
- `bash` is treated as destructive even for read-only commands (`git status`, `cat`). That's
  intentional (can't parse intent). The agent should use `read`/`grep`/`ls`/`find` tools for
  inspection while children run. State this in the report so reviewers know it's deliberate.

## Report

`_slop/report-23/6_parent_freeze_rule.md`: the hook + endpoint code, the five verification
transcripts (chat ids + the turn timestamps proving ordering), the probe latency you
measured, and confirmation that spawn parallelism and the slot limit are untouched.
