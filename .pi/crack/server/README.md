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
