I'll start by examining the relevant code files to understand the current structure and then formulate a plan.



Now I have a clear picture of the codebase. Let me formulate my understanding and plan.


Now I have a complete picture. Let me formulate my hypotheses and plan.

---

## Hypotheses about User Intent

1. **Primary goal**: Add a subtle footer note at the bottom of the task page (`/tasks/{task_id}`) showing the server name (e.g., "crack-pi-server" or similar)
2. **Scope**: Footer should appear on all task pages (the `/tasks/{task_id}` route), not on the homepage or other pages
3. **Style**: Small, muted text, centered or right-aligned at the bottom of the main content area
4. **Server name**: Likely "crack-pi-server" (the server's title from `FastAPI(title="crack-pi-server")`) or configurable

---

## Verification Approach

Since the server runs live in Docker at `http://localhost:9847` with hot reload:
1. **Manual verification**: Visit `http://localhost:9847/tasks/<task_id>` after changes, verify footer appears at bottom of main content
2. **HTML inspection**: `curl` the task page and verify footer HTML is present in the response
3. **Hot reload verification**: Edit `app.py`, wait ~1s, refresh page — no rebuild needed

---

## Lay of the Land

| Location | Purpose | Relevant Lines |
|----------|---------|----------------|
| `src/crack_server/app.py` | Main app, routing, HTML rendering | |
| `_render_base()` | Base HTML template wrapper | 130–190 |
| `task_page()` | Task page route handler | 451–482 |
| `_render_task_header()` | Task header with title, ID, created/modified | 378–418 |
| `src/crack_server/static/app.css` | All custom styles | 1–24 (and more) |
| `src/crack_server/static/app.js` | htmx/tab JS | 1–10 (and more) |

**Current HTML structure** (from `_render_base`):
```html
<body data-task-id="...">
  <main>
    {body}  ← task_page() injects header + prompt list + stage tabs here
  </main>
  <script src="/static/app.js"></script>
</body>
```

**Where footer should go**: Inside `<main>`, after `{body}`, before `</main>` — so it appears below stage panels but within the main content area.

---

## Plan

### Option A: Footer in `_render_base` (global, appears on all pages)
- Modify `_render_base()` to accept an optional `footer` parameter
- Pass footer content from `task_page()` only
- **Pro**: Reusable for other pages; **Con**: Slightly more invasive change

### Option B: Footer inline in `task_page()` body (task-page only)
- Append footer HTML directly in `task_page()`'s body string
- **Pro**: Minimal change, scoped to task page; **Con**: Not reusable

### Option C: Footer via CSS `::after` on `<main>` (pure CSS)
- Add `main::after { content: "crack-pi-server"; ... }` in `app.css`
- **Pro**: Zero Python changes; **Con**: Content in CSS, not semantic HTML

---

## Recommended Approach: **Option A** (minimal, clean, reusable)

1. **Modify `_render_base()`** (lines 130–190): Add optional `footer: str = ""` parameter; inject `<footer>{footer}</footer>` inside `<main>` after `{body}`
2. **Modify `task_page()`** (lines 451–482): Pass `footer=_render_server_footer()` where `_render_server_footer()` returns a small styled fragment
3. **Add CSS** in `app.css`: Style `main > footer` as small, muted, centered text with top margin

---

## Clarifying Questions


I've analyzed the codebase and identified the key locations. Before finalizing the plan, I need clarification on three design decisions:


Waiting for your answers to the three questions above — they'll determine the exact implementation approach.