# Plan: verify subagent-stop-plan + Pico-CSS UI revamp

## Context

Two jobs in one pass:

1. **Verify** that `_slop/subagent-stop-plan.md` (async worker + `wait_join` + `ask_user`) was implemented correctly from the staged diff.
2. **Revamp the crack-server web UI** onto class-based Pico CSS v2 with a persistent left-hand nav sidebar, replacing all the ad-hoc inline styles and misused `.secondary` "red button" hacks with proper Pico constructs, then document the new CSS situation.

All investigation/testing runs **inside the container** via `docker exec crack-dev …` or the browser MCP tools (which already run in-container). The dev server listens on port `9847`.

---

## Part 1 — Verification result (already done; two fixes to apply)

The subagent-stop-plan implementation is **substantially complete and high-fidelity across all three phases** — Phase A (single asyncio process, worker→async, `pi_proc` port, async propagation), Phase B (`signals.py`, `wait.py`, wait route + `wait_join` tool, `waiting_on` watchdog credit, two-strike rebuild), and Phase C (`ask_user` hop-terminating, `awaiting_user` phase, answer→resume flow). **The full suite passes: 66 passed** (`docker exec crack-dev sh -lc 'cd /workspace/.pi/crack/server && uv run python -m pytest -q'`).

Two real defects to fix as part of this work:

- **A. Duplicated prompt block.** In `.pi/crack/sub_agents/{coder,explorer,planner,tester}/system.md`, the `## Coordinating sub-agents and the human` section (heading + two bullets) is appended **twice, back-to-back**. Delete the second copy in all four files (leave one copy each).
- **B. Undeclared async test dependency.** `pyproject.toml` `dev = ["pytest>=8"]` has no `anyio`/`pytest-asyncio`; the `@pytest.mark.anyio` tests only pass because `anyio` ships transitively via FastAPI. Add `anyio` explicitly to the `dev` group (e.g. `dev = ["pytest>=8", "anyio>=4"]`), then `uv sync` and re-run the suite to confirm still-green.

Non-blocking deviations (no action, note only): `test_plan41.py` was left unchanged (legitimate — it drives the sync `run_agent_hop`/`run_pi_text` wrappers that now delegate to the async impl); planner's own grill flow and the "max concurrent non-waiting hops" knob remain out of scope per the plan.

---

## Part 2 — Pico-CSS UI revamp

### Facts about the current UI
- **No templates** — all HTML is inline Python f-strings.
- **One shell:** `ui.py:_render_base` (lines 58–86) builds `<head>` + `<body><main>{body}</main>`. It currently loads **`pico.classless.min.css`** (the classless build, so `.secondary`/`.primary`/`.container` are inert) + `/static/app.css`.
- **One stylesheet:** `src/crack_server/static/app.css` (507 lines).
- The remembered "red `.secondary`" is actually **inline `style="color:#c44"`** layered on `class="secondary"` (`ui.py:226`, `routes_tasks.py:69`), plus the custom `.stop-btn`/`.chat-stop` red rules (`app.css:444`). No `.secondary` rule exists.
- Full HTML pages (each calls `_render_base`): `/` (`routes_tasks.py:145`), `/tasks/{id}/view/{slug}` (`routes_tasks.py`), `/chats/{id}` (`routes_chats.py:25`), `/stages/{slug}` (`routes_stages.py:253`), `/sub_agents` + `/sub_agents/runs/{id}` (`routes_sub_agents.py:392,429`).
- Home-page nav source: harness-stage `<ul>` + Sub-agents link (`routes_tasks.py:158-190`), the task cards, and the chats section (`chats.py:99-109`).

### Decisions (confirmed with user)
- Destructive buttons (STOP, Delete, Remove) → Pico **`class="contrast"`** (drop `.stop-btn`/`.chat-stop` + inline red).
- **Force light theme**: `<html lang="en" data-theme="light">` — matches current look, avoids dark-mode clashes with the remaining hardcoded code-block colors.
- Load order: Pico first, then `/static/app.css` (layout + page-wide customizations only, no duplication of Pico).

### 2a. Shell + layout (`ui.py:_render_base`)
- Swap the Pico `<link>` to the class-based build:
  `https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css`
- Add `data-theme="light"` to `<html>`.
- Restructure the body into a two-pane layout:
  ```html
  <body{task_attr}>
    <div class="layout">
      <aside class="sidebar">{_render_sidebar()}</aside>
      <main class="container-fluid">{body}</main>
    </div>
    <script src="/static/app.js"></script>
  </body>
  ```
- New helper **`_render_sidebar()`** in `ui.py` (lazy-import `stages`, `paths`, `chats` inside the function to avoid import cycles) — a Pico `<nav>` of `<a>` links, compressed, built from the same data as the home page:
  - **Home** link (`/`).
  - **Tasks** — one `<a href="/tasks/{id}">` per `paths.list_task_ids()`.
  - **Harness Stages** — one `<a href="/stages/{slug}">` per `stages.REGISTRY`, plus **Sub-agents** (`/sub_agents`).
  - **Chats** — one `<a href="/chats/{id}">` per chat (reuse/extract a chat-id lister from `chats.render_home_section`; add a small `chats.list_chat_links()` helper if only the full-section renderer exists).
  - Group with `<small>`/`<h6>` headers; all nav items are `<a>` elements (per the requirement).

### 2b. `static/app.css` rewrite
Replace the `main { max-width:52rem }` column with the sidebar layout; strip everything Pico now provides; keep only genuine layout/structural + page-wide rules.

- **Layout (new):**
  - `.layout { display:flex; align-items:flex-start; }`
  - `.sidebar { flex:0 0 400px; max-width:400px; height:100vh; position:sticky; top:0; overflow-y:auto; padding:1rem; border-right:1px solid var(--pico-muted-border-color); }`
  - `main.container-fluid { flex:1; min-width:0; }` (Pico gives it padding; content stays full-width)
  - Responsive: below ~768px, `.layout { flex-direction:column }` and `.sidebar { flex-basis:auto; height:auto; position:static; }`.
- **Delete (Pico covers these):** `main{max-width}`; card borders/padding done via inline `style` on `<article>` (`routes_tasks.py:59`) → rely on Pico `<article>`; input/textarea `width:100%` rules (Pico forms are already full-width); redundant `class="primary"`.
- **Buttons:** delete `.stop-btn`/`.chat-stop` rules; the emitters use `class="contrast"`. Remove inline `style="color:#c44;border-color:#c44"` on Remove/Delete → `class="contrast"` (or `"contrast outline"` for lighter weight).
- **Muted text:** replace scattered inline `style="color:#666"` with a single tiny helper `.muted { color: var(--pico-muted-color); }` (page-wide customization Pico has no utility class for), or `<small>` where semantically apt.
- **Keep as genuine structural CSS** (no Pico equivalent), but re-point hardcoded colors to Pico vars (`--pico-muted-border-color`, `--pico-muted-color`, `--pico-card-background-color`): `.stage-tabs`/`.tab*`, `.stage-panel`, `.explore-actions` table, `.explore-summary`/`.plan-final`/`.plan-todo` accent borders, `.plan-question` fieldsets/`.plan-options`, `.approval-controls`, `.stage-error`/`.error-log`, `.user-prompt-msg`, `.part-row`, `.htmx-indicator`, `.task-card` transition, `.chat-form`. The dark code-block backgrounds (`#2b2b2b`) stay (intentional, light-theme-forced).

### 2c. Sweep inline styles in the HTML builders
Across `routes_tasks.py`, `ui.py`, `routes_sub_agents.py`, `chats.py`, `stages/render.py`:
- Drop `style="color:#666"` → `.muted` / `<small>`.
- Drop `class="secondary"` where it meant "danger" → `class="contrast"`; keep `class="secondary"` only where a genuine muted/secondary action is intended (e.g. Cancel).
- Small flex rows (`.actions`, `.stage-buttons`, `.title-row`, button groups) stay as custom layout classes; where it's a labeled two-column form row, consider Pico `role="group"` / `.grid`. Do **not** over-convert structural fieldset/table markup.
- The `← Home` / `← All tasks` ad-hoc back-links become redundant once the sidebar is persistent — keep or drop per page; at minimum they can stay as plain `<a>`.

### 2d. Documentation
- **`.pi/crack/server/README.md`** — add a "Styling (Pico CSS)" section: class-based Pico v2 via the `pico.min.css` CDN link; forced light theme; the sidebar/`container-fluid` layout; that `static/app.css` holds **only** layout logic + page-wide customizations (never duplicating Pico); destructive buttons use `.contrast`; links to `https://picocss.com/docs`, `/docs/button`, `/docs/container`, `/docs/nav`.
- **`.pi/crack/server/AGENTS.md`** — update line 3 ("pico.css app") to state **class-based** Pico v2 + light theme + sidebar shell in `ui.py:_render_base`, and add a short convention note near the `static/app.css` reference: use Pico classes/`--pico-*` vars, don't hand-roll colors/borders Pico already provides.

### Critical files
- `src/crack_server/ui.py` (shell + new `_render_sidebar`)
- `src/crack_server/static/app.css` (rewrite)
- `src/crack_server/routes_tasks.py`, `routes_sub_agents.py`, `routes_stages.py`, `chats.py`, `stages/render.py` (inline-style/button sweep)
- `.pi/crack/server/README.md`, `.pi/crack/server/AGENTS.md`
- `.pi/crack/sub_agents/{coder,explorer,planner,tester}/system.md` (de-dup — Part 1A)
- `.pi/crack/server/pyproject.toml` (anyio dep — Part 1B)

---

## Verification

All in-container (`docker exec crack-dev …`) or via browser MCP.

1. **Tests unchanged-green:** `docker exec crack-dev sh -lc 'cd /workspace/.pi/crack/server && uv sync && uv run python -m pytest -q'` → expect 66 passed (confirms Part 1B dep + system.md edits didn't break anything).
2. **Server up:** confirm `crack-server` is serving on `:9847` (it runs from the container start script); restart if needed.
3. **Visual check with browser MCP** (chromium or firefox tools — they run in-container): load `/`, a `/tasks/{id}/view/{slug}`, `/chats/{id}`, `/stages/{slug}`, `/sub_agents`, and a run page. Screenshot each and verify:
   - Left sidebar ~400px wide, vertically scrollable, showing Home + Tasks + Harness Stages + Sub-agents + Chats as `<a>` links, present on **every** page.
   - Right pane is full-width content.
   - Buttons render as real Pico buttons; STOP/Delete/Remove are `contrast` (not the old inline red); no unstyled/inert `.secondary`.
   - No dark-mode clashes (light theme forced).
4. **Grep clean-up check:** no remaining `style="color: #666"`, `.stop-btn`/`.chat-stop`, or `pico.classless` references.
