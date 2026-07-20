# crack-pi-server

Serves a small HTML editor for markdown prompts under `.pi/crack/tasks/<task_id>/`.

```bash
# from repository root
cd .pi/crack/server
uv sync
uv run crack-server
```

Environment:

- `CRACK_PI_PROJECT_ROOT` — project root (default: current working directory when started)
- `CRACK_PI_PORT` — listen port (default: `9847`)
- `CRACK_PI_HOST` — bind address (default: `127.0.0.1`)

## Styling (Pico CSS)

The UI uses **class-based Pico CSS v2** from the CDN:

`https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css`

Loaded first in `ui.py:_render_base`, followed by `/static/app.css`. The shell forces
light theme (`<html lang="en" data-theme="light">`) and a two-pane layout: a sticky
~400px left sidebar (`aside.sidebar`) plus a full-width content pane
(`main.container-fluid`).

`static/app.css` holds **only** layout logic and page-wide customizations — never
duplicate what Pico already provides (buttons, forms, articles, containers, muted
borders). Prefer Pico classes and `--pico-*` CSS variables.

Destructive actions (STOP / Delete / Remove) use Pico's `class="contrast"`. Secondary
muted actions (Cancel, Regenerate Title) keep `class="secondary"`.

Docs: [Pico CSS](https://picocss.com/docs), [Buttons](https://picocss.com/docs/button),
[Containers](https://picocss.com/docs/container), [Nav](https://picocss.com/docs/nav).
