# Crack harness — model controls, switch visibility, and improvement backlog

This document records (1) what shipped this turn against your request, and (2) a
grilled backlog of further improvements, each tagged **DO-NOW** (small, safe,
high-value — worth a follow-up turn) or **LATER** (needs a decision, a design
pass, or is speculative). The goal is a more robust coding system without
gold-plating.

---

## 1. Shipped this turn

- **Plan is first-message-only (correctness fix).** A plan chat re-derived its
  phase from *each exchange's own turns*, so every new user message restarted
  planning from scratch. Now the chat **graduates** once the first message's
  resolution completes (without an outstanding `ask_user`), and every later
  message runs unrestrained. `info["graduated"]` is the sticky gate.
  Files: `chats.run_chat`, `chat_engine` unchanged (still plan-agnostic).
- **New-chat form toggle.** Plan checkbox (default on) shows the two
  planner/implementer dropdowns; unchecking reveals a single model dropdown.
  Pure delegated JS (`initPlanToggle`), defaults seeded from `settings`.
- **Continuation model dropdown above Send.** After graduation (and always for a
  non-plan chat) the Send form carries a `model` dropdown, default = implementer
  model (plan) / non-plan model. `post_message` now records it on the exchange
  (`_pop_pending` copies it through); before graduation the pairing stays a
  read-only badge.
- **Model-switch visibility (100%).** Every persisted turn now records its
  `model` (`make_turn` + `TurnPersister.current_model`, stamped before each hop
  in both the chat and sub-agent loops). The trajectory shows a **per-turn model
  tag** and a **divider row** at each switch — "prewalk plan complete →
  implementing on X" when a todo preceded the switch (the auto-swap), else
  "switched model → X" (a user switch). Threaded via a shared `model_state`
  across exchanges so cross-message switches show too.
- **Right agent tree / card model sync.** Cards and the right tree now display
  the run's *phase* model (`_run_display_model` / `_chat_display_model` via
  `prewalk.model_for_phase`) instead of the static persona-config default, so
  the badge tracks planner→implementer.
- **CSS/table polish.** Bordered+padded top-level `#chat-msgs` frame colored
  like the sub-agent cards (blue while running, red on error); fixed-column
  `explore-actions` table (no more ragged alignment); tool output shows an
  inline ~8-line preview with a **single expand icon** (⤢) instead of a text
  `output` toggle; styles for the model tag, switch divider, and form groups.
- **Tests.** `tests/test_model_switch.py` (17 cases) covers model recording,
  divider/tag rendering, output preview, the graduation-gated form, and
  tree-model sync. Full suite: 109 passed.

> ⚠️ Not visually verified in a live browser — the local server on `:9847` was
> accepting connections but not responding when I checked, so the CSS was written
> from the markup, not a screenshot. **DO-NOW: eyeball the chat + a sub-agent
> card once the server is back** (particularly the fixed table widths at
> `7.5rem`/`6.5rem` — bump if long tool names like `analyze_image` wrap ugly, and
> confirm the `#chat-msgs` border doesn't double up with nested sub-agent cards).

---

## 2. Grilled backlog

### Data-flow / correctness

- **DO-NOW — `ask_user` during planning is a soft edge.** Graduation keys off
  "no `pending_question` after the exchange," so a planner question keeps plan
  mode alive correctly. But if the planner asks, the *answer* exchange runs with
  `plan_active` still true and re-enters `PLANNING` on an empty turn list — fine,
  but the divider/tag will read as planner even though the session already holds
  the plan. Low harm; worth a targeted test with a fake `ask_user` in planning.
- **LATER — collapse the two state shapes.** Chat `exchanges[idx].turns` vs
  sub-agent top-level `turns` still diverge (the prewalk plan explicitly deferred
  this). The model-switch rendering had to thread `model_state` differently for
  each. A single turn-list shape would delete a class of "which renderer" bugs.
- **DO-NOW — persist the *reason* a hop ended on the turn.** We record `model`
  now; also recording `reason` (`swap`/`agent_end`/`time_cap`/`nudge`) would let
  the trajectory explain *why* the next turn exists (e.g. show the nudge inline),
  which today is invisible.

### UI / trajectory

- **DO-NOW — the divider can't distinguish a genuine user switch from a
  same-model resend.** It only fires on model *change*, which is correct, but a
  user resuming on the *same* model shows nothing — fine. However a user who
  picks the planner model again post-graduation gets "switched model" with no
  hint plan mode won't re-engage. Add a one-line muted note on the form: "plan
  mode is locked for this chat — start a new chat to plan again."
- **DO-NOW — output preview loses ANSI / very-wide lines.** The 8-line clamp is
  by `max-height`, so a single 500-col line still overflows horizontally inside
  the cell. Wrap `.tool-out-preview` is set, but verify wide `bash` output
  scrolls rather than stretches the table.
- **LATER — collapse long trajectories.** A 25-hop coder run renders every turn
  open. Consider auto-collapsing all but the last N turns behind a "show earlier
  N turns" toggle, matching how the sub-agent card collapses when done.
- **LATER — per-turn cost/token line.** `context_stats` already reads session
  usage; surfacing per-turn tokens next to the model tag would make the
  planner→implementer cost win from prewalk legible (the whole point of prewalk).

### Model / settings

- **DO-NOW — validate model ids against the cache on save.** `_plain_model_select`
  keeps an unknown saved value as an option, so a typo'd settings model silently
  becomes a broken chat. A soft warning ("not in `pi --list-models`") on the
  settings + new-chat forms would catch it before a run fails.
- **LATER — let a chat change its planner/implementer pairing before the first
  message.** Right now the pairing locks at creation (from the new-chat form). If
  you open a chat and realize the models are wrong, you must delete + recreate.
  Cheap: show the plan pairing as editable dropdowns until the first message.

### Sub-agents / robustness

- **DO-NOW — the right-tree root badge now calls `prewalk.model_for_phase` every
  render.** It reads the latest exchange's turns; for a very long chat that's a
  list copy per 2s poll. Negligible now, but if chats grow, cache the display
  model on `info` at graduation.
- **LATER — surface sub-agent model switches in the right tree, not just the
  card.** The tree shows the current phase model; it does not show that a swap
  *happened*. A tiny "⇄" marker when `swapped_already(turns)` would mirror the
  trajectory divider.
- **LATER — retry should re-plan or not, explicitly.** `retry()` resumes the
  session; for a plan run that already swapped, retry correctly stays on the
  implementer (phase derives from turns). Worth a test pinning that a retried
  swapped run does not re-inject the planning append.

### Testing / tooling

- **DO-NOW — one end-to-end graduation test** with `fake_pi`: first message
  plans+swaps, second human message runs on the picked continuation model and
  `info["graduated"]` is set. The current tests cover the helpers but not the
  `run_chat` wiring.
- **LATER — a tiny visual-regression harness.** Since CSS is hand-written against
  markup, a screenshot diff of the chat + card pages (Playwright, the MCP browser
  tools) on each change would catch alignment regressions like the one you
  flagged.

---

## 3. Recommended next turn

If you want a focused follow-up, the highest-value **DO-NOW** cluster is:
live-browser eyeball of the new CSS, the end-to-end graduation test, per-turn
`reason` recording, and the "plan is locked" form note. The rest can wait for a
decision.
