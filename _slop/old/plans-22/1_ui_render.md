# Plan 1 — UI rendering & CSS (trajectory, spawn/todo cards, fonts, cursor meter)

**Scope:** pure presentation. No sub-agent lifecycle, no worker/async changes. Everything here lives in
`.pi/crack/server/src/crack_server/render.py`, `.../context_stats.py`, and `.../static/app.css`.
All work is in the Python server at `/home/p/VIDOEGAME/crack/.pi/crack/server`.

**How to run tests (memory):** from the server dir, `python -m pytest -q`. Use the project venv
(`.venv`). The render helpers are pure functions — you can unit-test them directly by importing
`crack_server.render`.

**Orientation (read these first):**
- `render.py` — `_render_tool_action_row` (the `else` branch at ~line 172 dumps unknown tools raw),
  `render_actions_table` (~line 285, builds the `<table class="explore-actions">`), `_clean_turn_text`,
  `_fmt_chars`.
- `ui.py` — `_esc`, `_render_markdown` (CommonMark via markdown-it, raw HTML disabled), `_load_template`.
- `context_stats.py` — `session_usage` and `render_context_line`.
- `static/app.css` — `.explore-actions` rules at lines 262–304.

---

## Task 1.1 — Trajectory "Path / command" column ~2× wider

**Problem.** `render_actions_table` (render.py ~285) emits a 3-column table (`Type`, `Path / command`,
`Size`). CSS gives col1 (`td:first-child`) and col3 (`td:last-child`) `white-space: nowrap`; the middle
column takes the remainder but there is no explicit weighting, so when middle content is short the column
reads as narrow. Requirement: middle column should get ~2× the relative width of the others.

**Approach (colgroup + fixed weights).** Deterministic and doesn't fight the `nowrap` cells:
1. In `render_actions_table`, inject a `<colgroup>` right after `<table ...>`:
   `<colgroup><col class="col-type"><col class="col-path"><col class="col-size"></colgroup>`.
2. In app.css add, near the `.explore-actions` block (~262):
   ```css
   .explore-actions { table-layout: fixed; }
   .explore-actions .col-type { width: 12%; }
   .explore-actions .col-path { width: 76%; }
   .explore-actions .col-size { width: 12%; }
   ```
   With `table-layout: fixed`, col1/col3 no longer auto-size to content — verify the `Size` cell
   (`in 1.2k / out 3.4k · 4.1s`) still fits at 12%; if it clips, bump to 14/72/14. Keep
   `td:last-child { white-space: nowrap; text-align: right; }` but add `overflow: hidden; text-overflow: ellipsis;`
   to col1/col3 so fixed layout never overflows the row.
3. The middle column already has `word-break: break-word` via `.explore-actions pre`; keep it.

**Verify:** `python -m pytest -q`. Then a render assertion — add or extend a test importing
`render.render_actions_table([...])` and assert the output contains `<colgroup>` and `col-path`.
Manual: load a chat page with a trajectory, confirm the middle column is visibly the widest.

---

## Task 1.2 — `spawn_coder` row: field-extracted pretty card (not raw `<pre>`)

**Problem.** A `spawn_coder` (or any `spawn_<slug>`) tool call falls into the generic `else` in
`_render_tool_action_row` (render.py ~172) and dumps the raw JSON `input` into `<pre class="cmd">` with
**no truncation** (unlike other rows). The tool's persisted block is:
`block["name"] == "spawn_coder"`, `block["input"] == {"instructions": <str>, "plan": <bool>}`,
`block["output"] == "Spawned coder run ... "` (confirmed from `.pi/extensions/crack/index.ts` `PARAMS`).

**Requirement.** Extract fields and pretty-render:
- Show a `plan: on` / `plan: off` label **above** the instructions.
- Render `instructions` markdown→HTML, showing only the **first 7 lines**, with a "full output" toggle
  (`<details>`) revealing the complete instructions.
- Keep the tool dot + `in/out` size cell like other rows.

**Approach.** In `_render_tool_action_row`, add a branch **before** the generic `else`:
```python
elif name.startswith("spawn_"):
    action_type = esc(name)
    plan_on = bool(args.get("plan"))
    plan_badge = (f'<span class="spawn-plan spawn-plan--{"on" if plan_on else "off"}">'
                  f'plan {"on" if plan_on else "off"}</span>')
    instructions = str(args.get("instructions") or "")
    middle = _render_clamped_markdown(instructions, max_lines=7,
                                      header=plan_badge, full_label="full prompt")
```
Add a small shared helper (reused by Task 1.3):
```python
def _render_clamped_markdown(md_text, max_lines, header="", full_label="full output"):
    """First `max_lines` lines rendered as markdown, with a <details> holding the
    full markdown render. `header` is emitted above (e.g. the plan badge)."""
    lines = md_text.splitlines()
    head = "\n".join(lines[:max_lines])
    head_html = _ui._render_markdown(head)
    body = f'{header}<div class="md-clamp-head">{head_html}</div>'
    if len(lines) > max_lines:
        full_html = _ui._render_markdown(md_text)
        body += (f'<details class="md-clamp-more"><summary>{_ui._esc(full_label)}</summary>'
                 f'<div class="md-clamp-full">{full_html}</div></details>')
    return f'<div class="md-clamp">{body}</div>'
```
Note: markdown-it output is trusted (raw HTML disabled in `_render_markdown`), so no double-escape.
For the spawn row, `block["output"]` (the "Spawned run …" text) still appends via the existing
`if output: middle += _render_tool_output(output)` path — keep it; it's the small run link/confirmation.

**CSS.** Add in app.css:
```css
.spawn-plan { display:inline-block; font-size:0.72rem; padding:0 0.35em; border-radius:0.25rem;
  margin-bottom:0.25rem; }
.spawn-plan--on  { background: var(--pico-ins-color, #1a7f37); color:#fff; }
.spawn-plan--off { background: var(--pico-muted-border-color); color: var(--pico-muted-color); }
.md-clamp { font-size:0.85rem; }
.md-clamp-head p:first-child, .md-clamp-full p:first-child { margin-top:0; }
.md-clamp-head { overflow:hidden; }
```

**Verify:** unit test — `render._render_tool_action_row({"name":"spawn_coder","input":{"instructions":"# Big task\n"+"\n".join(f"line {i}" for i in range(20)),"plan":True},"output":"Spawned coder run X"})` and assert: contains `plan on`, contains `md-clamp-more` (because >7 lines), does **not** contain the raw JSON `{"instructions"`. `python -m pytest -q`.

---

## Task 1.3 — `todo` row: markdown pretty-print, section font 80%

**Problem.** The `todo` tool block (`name == "todo"`, `output` = the plain `renderTodos` text like
`[x] #1 …\n[ ] #2 …`) also hits the generic `else` and shows a raw `<pre>` with no clamp.

**Requirement.** Parse output markdown→HTML, pretty display only (drop the `<pre>`), and set the whole
todo section font-size to 80% of normal.

**Approach.** Add a branch before the generic `else` in `_render_tool_action_row`:
```python
elif name == "todo":
    action_type = "todo"
    out_text = str(block.get("output") or "")
    middle = f'<div class="todo-render">{_ui._render_markdown(out_text)}</div>'
    # Do NOT fall through to the generic `if output: middle += _render_tool_output(output)`.
    ...
    return f"<tr><td>{type_cell}</td><td>{middle}</td><td>{size}</td></tr>"
```
Because the generic tail (`if output:`) is shared, guard it: only append `_render_tool_output(output)`
when `name` is not in `("todo",)` (spawn keeps its output; todo does not — its output *is* the middle).
Cleanest: give `todo` its own early `return` inside the branch so it skips the shared tail entirely.

**CSS.** `.todo-render { font-size: 0.8rem; }` — this is the "80% of normal" for the todo section.
The `renderTodos` text uses `[x] #n text` lines; markdown-it renders these as a paragraph with line
breaks. If you want checkboxes styled, that's optional polish — the requirement is only md→html + 80%.

**Verify:** unit test — todo block output `"[x] #1 done\n[ ] #2 open"` renders to a `todo-render` div,
no `<pre`, `python -m pytest -q`.

---

## Task 1.4 — Global app font size → 85%

**Problem.** Pico's default text is too large.

**Approach.** Set the root font-size so every `rem`-based size scales down uniformly (all the component
sizes in app.css are already in `rem`, so this is a single clean lever). At the **top** of app.css:
```css
:root { font-size: 85%; }   /* shrink all rem-based sizing; was pico default 100% */
```
Check nothing in the app uses `px` font-sizes that would escape this (grep app.css for `px` on
`font-size` — none currently). Confirm the sidebar, chat form, and tables still lay out (85% is mild).

**Verify:** grep `static/app.css` contains `:root { font-size: 85%` (or equivalent first rule). Load the
app; confirm overall text is ~15% smaller and no layout breaks. No pytest needed (pure CSS).

---

## Task 1.5 — Cursor context/price line (estimate used tokens, hide $0)

**Decision (locked):** cursor-style drivers report `usage.input == 0`, `usage.cacheRead == 0` always
(only `output` is populated), and `usage.cost.total == 0` (subscription). The context **window** *is*
known (200K in the models cache). So: **estimate used tokens from the session transcript**, and **hide
the price** when cost is 0.

**Where.** `context_stats.py`. Today `session_usage` returns `None` when `tokens <= 0`
(`input + cacheRead`), which is why the whole line vanishes for cursor.

**Approach.**
1. Add an estimator that reads the newest session file and approximates prompt tokens from character
   volume. The pi session jsonl stores every message; a cheap, good-enough estimate of the *current
   context size* is `chars_of_last_prompt / 4`. Simplest robust proxy: sum the character length of all
   message `content` in the session up to and including the last assistant message, divide by 4. Reuse
   `_read_tail_lines`/`_newest_session`; you already read the tail. Implement:
   ```python
   def _estimate_context_tokens(sessions_dir: Path) -> int | None:
       session = _newest_session(sessions_dir)
       if session is None: return None
       total_chars = 0
       for line in _read_tail_lines(session):   # tail is enough for a rough gauge
           try: obj = json.loads(line)
           except Exception: continue
           msg = obj.get("message") if isinstance(obj, dict) else None
           if not isinstance(msg, dict): continue
           c = msg.get("content")
           if isinstance(c, str): total_chars += len(c)
           elif isinstance(c, list):
               for part in c:
                   if isinstance(part, dict): total_chars += len(str(part.get("text","")))
       return (total_chars // 4) if total_chars else None
   ```
   (Tail-only is acceptable: the meter is a gauge, not billing. Document the `~` approximation.)
2. In `session_usage`, when the newest usage row has `tokens <= 0` **but** `output > 0` (i.e. a driver
   that reports output but not input — cursor), return a dict flagged `estimated: True` with
   `tokens = _estimate_context_tokens(...) or 0`, keeping `output`, `cost`, `model`. Do **not** early-return
   `None` in that case. Keep the existing exact path for drivers that *do* report input.
3. In `render_context_line`:
   - When `usage["tokens"]` came from the estimate, prefix the label with `~` and title the meter
     "estimated (driver reports no input tokens)". Compute pct against the window as today.
   - Price: only render `cost_str` when `cost > 0`. When `cost == 0`, render nothing (no `$0.0000`).
     Optionally append a muted `subscription` tag when provider is `cursor-agent`
     (`models.model_info(model)["provider"] == "cursor-agent"`), but a blank is acceptable.

**Verify:** unit test with a temp sessions dir containing a jsonl whose assistant `usage` is all-zero
except `output`, and some message content. Assert `session_usage(dir)["estimated"] is True`,
`tokens > 0`, and `render_context_line(dir, "cursor-agent/composer-2.5")` contains `~` and the `200k`
window and **no** `$`. Also assert a normal driver (input>0) still returns exact (no `~`).
`python -m pytest -q`.

---

## Plan 1 completion checklist (automatic)
Run from `.pi/crack/server`:
```
python -m pytest -q
grep -q "col-path" src/crack_server/static/app.css
grep -q "font-size: 85%" src/crack_server/static/app.css
grep -q "spawn_" src/crack_server/render.py && grep -q "todo-render" src/crack_server/render.py
grep -q "estimated" src/crack_server/context_stats.py
```
All must pass. Then a manual page load of a chat with sub-agents to eyeball the four visual changes.


Use all uv, python, bash, etc. commands only under the following way: docker exec crack-dev bash -exc 'command .... ' . Do not run any code outside the container, you will not have the tools available at all. You can use rg, fzf, grep, inside the container only. You can use edit and read commands as normal. The container mounts the workspace dir at /workspace where your shells will spawn. 