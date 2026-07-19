# Plan 4.2 — Web transport & trajectory UI: long-poll, incremental updates, prompt visibility

One of three independent plans (4.1 runner/lifecycle, 4.2 this, 4.3 refactor + residual
bugs). This part owns the browser-facing layer: polling transport, fragment structure,
trajectory rendering (including the new "user prompt" rows), and `static/app.js`/`app.css`.
It touches `app.py` routes/renderers, `stages/base.py` rendering helpers, per-stage
`render_status` structure, and `chats.py` rendering. It does **not** change how agents
run (that is 4.1) and does not move files (that is 4.3). It is implementable standalone:
every feature degrades gracefully if 4.1 hasn't landed (details inline).

Fixes the two user-reported UI problems from `pi-crack-server-3-prompt-2.md`:
compiled prompts invisible in the trajectory, and the every-2s full-page-fragment reset
that collapses `<details>` and spams the server.

---

## 0. Root-cause notes (current tree)

- Every stage panel is one div re-rendered wholesale every 1.5s
  (`Stage.wrap_status`, `base.py:175-205`, `hx-swap="outerHTML"`): all `<details>`
  open-state lives in DOM nodes that get replaced, so everything collapses each tick
  even with zero new content.
- `app.js:30-45` `scrollToLatest` runs on **every** `htmx:afterSwap` — the
  `data-msg-count` attribute exists precisely to gate this but is never compared, so
  the viewport also jumps every tick.
- Independent 1.5s pollers stack up per page: stage status (`base.py:193`), auto-follow
  (`app.py:293`), title regen (`app.py:171`), chat status (`chats.py:201`) → the
  request spam in the logs.
- Rendering is expensive per tick: full markdown render + `_render_path_ref` re-reads
  referenced files from disk every poll (`s01_explore.py:54-72`).

## 1. Server: one long-poll change endpoint per page

Replace interval polling with **long-poll on state change**, keyed by file mtimes so it
needs nothing from plan 4.1.

- `paths.py`: `def task_state_mtimes(task_id) -> float` — max mtime over the task's
  known state files (`explore.json`, `plan.json`, `plan_review.json`,
  `implementation.json`, `impl_review.json`, `finished.json`, `title_regen.json`,
  `info.json`); `def chat_state_mtime(chat_id) -> float`.
- New route `GET /tasks/{task_id}/wait?since=<float>&slug=<slug>` (async def):
  loop up to 25s: if `task_state_mtimes(task_id) > since` **or** the follow frontier
  moved past `slug` (reuse `furthest_engaged_slug`, `app.py:228-240`), return
  immediately; else `await asyncio.sleep(0.3)`. Response is JSON:
  `{"since": <new mtime>, "redirect": "/tasks/…/view/<slug>"|null, "changed": bool}`.
  On timeout return `{"changed": false, "since": same}`. Must be `async def` so the
  30-ish parked connections don't eat the sync threadpool.
- Same for chats: `GET /chats/{chat_id}/wait?since=`.
- The auto-follow poller (`_render_stage_follow`, `app.py:279-294`) and the
  title-regen poller (`app.py:160-181`) are folded into this one wait loop: `redirect`
  covers follow; the title slot refetches only when `changed` (see §3).

**Result:** an idle page holds exactly **one** open request per 25s instead of ~120
requests/min across 3 pollers. Log spam gone.

## 2. Client: fetch-on-change, swap only deltas

Rewrite `static/app.js` around a small loop (plain JS, no htmx trigger polling):

```
async function watch(taskId, slug) {
  let since = document.querySelector('[data-state-mtime]')?.dataset.stateMtime || 0;
  for (;;) {
    const r = await fetch(`/tasks/${taskId}/wait?since=${since}&slug=${slug}`);
    const j = await r.json().catch(() => null);
    if (!j) { await sleep(2000); continue; }   // server restart etc.
    if (j.redirect) { location.assign(j.redirect); return; }
    since = j.since;
    if (j.changed) htmx.ajax('GET', statusUrl + `?after=${lastMsgIndex()}`, {target, swap});
  }
}
```

- htmx stays for forms/buttons; the *polling* `hx-trigger="every 1.5s"` attributes are
  removed from `wrap_status` (`base.py:193`), `_render_stage_follow`,
  `_render_title_regen_pending`, and `render_chat_content` (`chats.py:198-203`).

## 3. Fragment structure: append messages, never rebuild the transcript

Restructure `Stage.wrap_status` output into three sibling regions with stable ids:

```
<div id="{slug}-content" data-stage-status data-msg-count data-state-mtime>
  <div id="{slug}-msgs">   … one child per .stage-msg, each with id="{slug}-msg-{i}" …</div>
  <div id="{slug}-tail">   … spinner OR error card OR Q&A form OR buttons/message box …</div>
</div>
```

- New query param on the status routes (`stage_status`, `app.py:759-762`;
  `chat_status`, `app.py:940-944`): `?after=<n>`. With `after`, the response contains
  **only** messages `i > n` plus an out-of-band swap for `#{slug}-tail`
  (`hx-swap-oob="outerHTML"`). The client swaps the new messages `beforeend` into
  `#{slug}-msgs`. Without `after` (initial page load) the full structure renders as
  today.
- Implementation: each stage's `render_status` already builds `parts: list[str]`
  (e.g. `s01_explore.py:441-498`). Split every stage's render into
  `render_msgs(task_id) -> list[str]` (stable, append-only history: meta line,
  questions, one entry per turn, summary, Q&A history, chat exchanges) and
  `render_tail(task_id) -> str` (volatile: spinner/error/forms/buttons). `wrap_status`
  assembles them and handles the `after` slicing generically so the six stages don't
  duplicate the delta logic. **Append-only rule:** msgs must never change once
  emitted — anything that mutates (walkthrough markdown, revised plan) lives in the
  tail or in its own single msg slot that is only emitted once final (e.g. summary is
  appended when `done`).
- The actions table currently renders one `<table>` for *all* turns
  (`base.py:584-615`) — that block is the transcript, so it must become one
  table-per-turn (or per-message `<div class="stage-msg">` wrapping a small table) so
  turns can be appended individually. Visual style unchanged (`app.css` selectors
  already target `.explore-actions` rows).
- `<details>` state now survives automatically (existing nodes are untouched).
  Belt-and-braces: keep a tiny `app.js` map of open `<details>` per msg id restored
  after swaps, for tail re-renders that contain details (error cards).
- Path-ref sections (`s01_explore.py:469-474`): render **lazily** — emit a
  `<details hx-get="/tasks/{id}/fileref?path=…&start=…&end=…" hx-trigger="toggle once">`
  so file contents load on first expand instead of being re-read from disk on every
  render (kills the B20 render-cost half; new small route wraps
  `pi_runner.read_file_lines`).

## 4. Scroll behavior

`app.js`: replace the unconditional `scrollToLatest` with:

- Track `data-msg-count` per stage container; scroll only when the count **increases**.
- Only auto-scroll if the user was already within ~200px of the bottom (don't yank
  someone reading an earlier turn).
- Never scroll on tail-only swaps.

## 5. "User prompt" rows in every trajectory

Renders the contract defined in plan 4.1 §1 (`{"kind": "user_prompt", "compiled",
"original"?, "label"?, "template"?, "hop"}` entries inside `turns`). Degrades cleanly:
if 4.1 hasn't landed, no such entries exist and nothing renders.

- `base.render_actions_table` / the new per-turn renderer: a `user_prompt` entry
  renders as its own `.stage-msg` row, visually distinct (right-aligned or accent
  border, `app.css`), labeled `user prompt · {label}`:
  - collapsed summary line: first line of `original` if present, else of `compiled`;
  - expanded (`<details>`): when `original` is present show it first ("original
    message"), then the full `compiled` text under a nested
    `<details><summary>compiled prompt ({n} chars, template {template})</summary>` —
    "what goes in", verbatim, escaped, in a `<pre class="prompt-full">`.
- Chats (`chats.render_chat_content`, `chats.py:164-203`) and s06 finished chat: the
  existing "You:" bubble becomes the collapsed view of the same expandable row (the
  compiled `chat.md` prompt behind the toggle).
- Skip-unknown-kind guard everywhere a turn list is iterated
  (`render_actions_table`, `render_transcript_plaintext` in `pi_runner.py:357-372` —
  prompt entries must NOT be fed back into gate/summary transcripts; filter
  `turn.get("kind")` there too, otherwise prompts double-feed the nano models).

## 6. Chats + title slot on the same transport

- `render_chat_content` gets the same msgs/tail split + `?after=` delta and the `wait`
  loop; Stop button and form live in the tail.
- Title regen: `_render_title_regen_pending` (`app.py:160-181`) drops its own poller;
  `app.js` refetches `/tasks/{id}/title-regen-status` only when a `wait` cycle reports
  `changed` and the slot is in pending state. The status route keeps its
  auto-save-on-done behavior for now (its B15 problem is fixed in plan 4.3).

## 7. STOP button / message box styling (cooperation with 4.1)

Plan 4.1 adds functional-but-plain STOP buttons and stopped-state message forms. This
plan owns their placement/styling: STOP sits beside the spinner in the tail; the
message box renders under the buttons at the bottom of the tail in `stopped`/`error`
states, styled like `chats.render_chat_form` (`chats.py:136-161`). If 4.1 hasn't
landed, these controls simply aren't emitted (feature-detect: render STOP only when the
stage state contains the `stop_requested` key or status == "stopped"/"running" with a
pid file — keep the check tolerant).

---

## Acceptance / test plan (browser-driven, no real pi needed)

Use the chromium/firefox MCP tools against a locally running server + worker (fake
state files are enough: hand-write a task's `explore.json` with turns and flip its
mtime with `touch`).

1. **No reset:** open a running stage page, expand three `<details>`; append a turn to
   the state JSON externally; within a second the new turn pops in at the bottom, the
   three expanded details stay open, viewport only scrolls if at bottom.
2. **No spam:** with the page idle 60s, the server log shows ≤ 3 `wait` requests and
   zero status requests (`list_network_requests` confirms).
3. **Prompt rows:** hand-insert a `user_prompt` entry into turns → it renders as an
   expandable row showing original + compiled verbatim; gate/summary prompt text (in
   logs) does not contain the inserted entry.
4. **Follow:** while viewing the frontier stage, mark a later stage engaged in its
   state file → browser navigates to the later tab via the `redirect` field.
5. **Old states:** a pre-existing task with no `kind` entries renders identically to
   before (visual diff of the msgs region).
