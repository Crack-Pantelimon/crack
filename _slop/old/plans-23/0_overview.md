# plans-23 — Podman sandbox isolation for crack conversations

**Read this file first, every time.** Each numbered plan (`1_*.md` … `7_*.md`) runs in
its own cold session on a cheaper model. This overview is the shared context none of
them should re-derive.

## The goal in one paragraph

Every crack conversation (top-level chat) and every sub-agent must run its `pi`
process inside its **own podman container** ("sandbox") whose workspace, target, and
root filesystems are **copy-on-write (`:O`) overlays** over the real ones. Runs can no
longer collide or half-break the tree, and the harness can safely modify its own code.
`crack-dev` stays the single long-lived container that runs the crack-server + worker
and only issues small `pi` summary/title/vision calls locally; **all agentic `pi` runs
move into sandboxes**, driven by `podman exec` from crack-dev over a mounted host
podman socket (docker-out-of-docker → host-level sibling containers).

## Non-negotiable invariant: "overlay lower must be stable"

An overlayfs **lowerdir must not change while a container has it mounted** — doing so is
undefined behavior. Two consequences drive the whole design:

1. **The server must not write into any overlaid tree.** All harness state moves off the
   repo tree onto a shared volume `crack-harness-data` mounted at `/crack-harness-data`
   in every container (Plan 1). After that, the only thing under `/workspace` the server
   touches is read-only persona config.
2. **A parent must not mutate its tree while a child sandbox has it as a lower.** When an
   agent has running sub-agents, any destructive tool (`bash`, `edit`, `write`, any MCP)
   implicitly `wait_join`s first (Plan 6). Spawning stays free until the parallel limit.

## Locked design decisions (do not relitigate)

- **Rollout:** full — both top-level chats and sub-agents get sandboxed.
- **Networking:** shared podman network `crack-net`; crack-server binds `0.0.0.0`;
  sandboxes reach it by hostname `crack-dev` (extension `BASE` host from `CRACK_PI_HOST`).
- **Destructive gating (Plan 6):** ALL `bash` + ALL MCP/custom tools trigger the implicit
  `wait_join`. Free tools: `read`, `grep`, `ls`, `find`, `todo`, `wait_join`, `ask_user`,
  `analyze_image`, `spawn_*`.
- **Patch flow (Plan 4):** harness **auto-applies** each finished child's patch into the
  parent overlay in finish order. On conflict/failure, **leave git in the conflicted
  state**, hand the managing agent the full patch path, and tell it to resolve directly,
  fix the application, and continue.
- **Patch scoping:** baseline `git write-tree` at sandbox start, `git write-tree` at end,
  `git diff <base> <end>` = exactly that agent's delta (the overlay inherits the repo's
  pre-existing dirty state, so a naive `git diff` is wrong — this was verified live).

## Verified environment facts (already true — don't re-test from scratch)

- Host: rootless podman 6.0.1, overlay driver. `docker` is a podman-docker shim.
- `crack-dev` (Debian 13) already has `podman` 5.4.2 installed (baked in `_docker/Dockerfile`
  before `VOLUME /root`) and already mounts the host podman socket:
  `run.sh` sets `-v $XDG_RUNTIME_DIR/podman/podman.sock:/run/podman/podman.sock`,
  `-e CONTAINER_HOST=unix:///run/podman/podman.sock`,
  `-e CRACK_HOST_REPO_ROOT=/home/p/VIDOEGAME/crack`.
- From inside crack-dev, `podman ...` drives the HOST podman (verified: it lists the host
  containers including `crack-dev` itself).
- `:O` overlays isolate writes from source for **both bind mounts and named volumes**
  (verified). `upperdir=/…,workdir=/…` persists the overlay to a folder (whole copied-up
  files, not a diff).
- **Killing the `podman exec` client does NOT kill the process inside** (verified). Mid-run
  kills must be `podman exec <sbx> pkill -9 -f <session-id>`; full stop is `podman kill <sbx>`.
- pi exposes a `tool_call` extension event — *"Fired before a tool executes. Can block."* —
  async, per-tool-typed (`bash/edit/write` vs read-only `read/grep/find/ls`). This is the
  hook Plan 6 uses.

## Where things live (paths)

- Server code: `.pi/crack/server/src/crack_server/` (run with `uv` inside crack-dev).
- Extension (pi tools): `.pi/extensions/crack/index.ts`.
- Persona config: `.pi/crack/sub_agents/<slug>/` — **committed code, stays in /workspace**.
- Path helpers: `.pi/crack/server/src/crack_server/paths.py`.
- Hop runner + kill: `.pi/crack/server/src/crack_server/pi_proc.py`.
- Sub-agent spawn/finish: `.pi/crack/server/src/crack_server/sub_agents/`.
- Container scripts: `_docker/{Dockerfile,run.sh,build.sh,_cont_start.sh}`.

## How to run commands (MANDATORY)

You have **no tools on the host**. Run every `pi`, `uv`, `python`, `git`, `rg`, `podman`
command through crack-dev:

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && uv run pytest -x tests/test_foo.py'
```

`Read`/`Edit`/`Write` on files work normally (the repo is bind-mounted at `/workspace`).

## Rebuilding the container

If you change `_docker/Dockerfile` or `run.sh`, rebuild + restart:

```bash
cd /home/p/VIDOEGAME/crack/_docker && ./build.sh && ./run.sh
```

Add new `RUN` lines **just before `VOLUME /root`** in the Dockerfile so rebuilds stay fast.
`run.sh` recreates crack-dev (kills the running server) — expected.

## The standard end-to-end test: a nemotron sample chat

Every plan must finish by driving a real chat on the **nemotron 120B super** model in
**non-plan mode** and confirming the system still works. Recipe (adjust the task per plan):

```bash
docker exec crack-dev bash -exc '
  cd /workspace
  # 1. create a chat, capture its id from the 303 Location header
  CID=$(curl -s -D- -o /dev/null -X POST http://127.0.0.1:9847/api/chats \
        | grep -i "^location:" | sed "s#.*/chats/##" | tr -d "\r")
  echo "chat=$CID"
  # 2. send a trivial non-plan task on nemotron-120b-super.
  #    planner_model non-empty => config editor shown; plan omitted => plan_flag=False => NON-PLAN.
  curl -s -X POST http://127.0.0.1:9847/api/chats/$CID/messages \
    --data-urlencode "msg=Create a file /workspace/HELLO_SANDBOX.txt whose only content is the word PONG, then stop." \
    --data-urlencode "model=nvidia/nemotron-3-super-120b-a12b" \
    --data-urlencode "planner_model=nvidia/nemotron-3-super-120b-a12b" >/dev/null
  # 3. poll to completion (phase back to idle), timeout ~5 min
  for i in $(seq 1 60); do
    P=$(python3 -c "import json;print(json.load(open(\"/crack-harness-data/unscripted_chats/$CID/chat.json\"))[\"phase\"])" 2>/dev/null)
    echo "poll $i phase=$P"; [ "$P" = idle ] && break; sleep 5
  done
'
```

(Before Plan 1 migrates state, the chat.json path is
`/workspace/.pi/crack/unscripted_chats/$CID/chat.json` instead.) Pick **trivial** tasks —
you are validating plumbing, not model quality. Good tasks: "write PONG to a file",
"reply with 2+2 then stop", "run `git status` and summarize". Keep them one-shot.

## Reports (MANDATORY)

At the end of each plan, write a report to `_slop/report-23/<N>_<name>.md` covering:
what you changed (files + why), the exact commands you ran, the sample-chat transcript
location (`/crack-harness-data/unscripted_chats/<id>/`), what passed, what you could not
verify, and anything the next plan's agent must know. A smarter agent will review all
reports + trajectories together, so be concrete and honest about gaps.

## Safety

- Never `git commit`/`push` unless the plan says so. Leave changes staged/working.
- Clean up any `crack-sbx-*` / `crack-proto*` containers and `crack-harness-data`-adjacent
  scratch you create while testing (`podman ps -a`, `podman rm -f`).
- If crack-dev won't boot after your change, that's a failure — fix or revert before writing
  the report, and say so.
