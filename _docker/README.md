# crack-dev container: browsers, WebDriver, wasm-pack and MCP tooling

This document describes the browser/automation tooling installed in the `crack-dev`
Docker container (Debian 13 trixie, repo mounted at `/workspace`), how it was
verified live, and how it is reproduced by the `Dockerfile` in this directory.

## Installed software (verified versions)

| Tool | Version | Source |
|------|---------|--------|
| Chromium | 150.0.7871.124 | `apt` package `chromium` |
| ChromeDriver | 150.0.7871.124 | `apt` package `chromium-driver` (binary `chromedriver`) |
| Firefox ESR | 140.12.0esr | `apt` package `firefox-esr` (binaries `firefox-esr`, `firefox`) |
| geckodriver | 0.37.0 | upstream tarball `geckodriver-v0.37.0-linux64.tar.gz` -> `/usr/local/bin` (no Debian package exists) |
| wasm-pack | 0.15.0 | `cargo install wasm-pack` (`CARGO_HOME=/usr/local/cargo`, on `PATH`) |
| pi-mcp-adapter | 2.11.0 | `pi install npm:pi-mcp-adapter` |
| web-search-mcp | 0.3.1 @ commit `eeb03f88525cbf74c4019e59a3fea45a537a760b` | git clone to `/root/web-search-mcp` + `npm install` + `npx playwright install` + `npm run build` |
| chrome-devtools-mcp | 1.6.0 | via `npx` (drives system chromium) |
| @playwright/mcp | 0.0.78 (playwright 1.62.0-alpha) | via `npx`, plus `npx @playwright/mcp@0.0.78 install-browser firefox` |

Playwright browser builds in the container:
- web-search-mcp's playwright: chromium 139.0.7258.5 (`chromium-1181`), firefox 140.0.2 (`firefox-1489`) under `/root/.cache/ms-playwright`.
- @playwright/mcp's firefox 152.0.4 (`firefox-1534`) — separate, newer playwright revision.

## Live install commands (what was actually run)

```bash
# browsers + chromedriver (apt; geckodriver has NO debian package)
apt-get update && apt-get install -y chromium chromium-driver firefox-esr

# geckodriver (pinned upstream release)
curl -sL -o /tmp/geckodriver.tar.gz \
  https://github.com/mozilla/geckodriver/releases/download/v0.37.0/geckodriver-v0.37.0-linux64.tar.gz
tar -xzf /tmp/geckodriver.tar.gz -C /tmp
install -m 0755 /tmp/geckodriver /usr/local/bin/

# wasm-pack
cargo install wasm-pack

# pi MCP adapter extension
pi install npm:pi-mcp-adapter

# web-search-mcp (pinned commit)
git clone https://github.com/mrkrsl/web-search-mcp /root/web-search-mcp
cd /root/web-search-mcp
git checkout eeb03f88525cbf74c4019e59a3fea45a537a760b
npm install
npx playwright install        # prints host-requirement warnings on trixie; browsers still launch fine
npm run build                 # produces dist/index.js

# playwright firefox build for @playwright/mcp (separate playwright revision)
npx -y @playwright/mcp@0.0.78 install-browser firefox
```

`npx playwright install` prints `BEWARE: your OS is not officially supported by
Playwright; downloading fallback build for ubuntu20.04-x64` and a host-requirement
validation error on Debian 13 — harmless: both playwright chromium and firefox
launch headless successfully (verified). The needed shared libraries are already
present because apt's chromium/firefox-esr pull them in.

## MCP configuration

The MCP servers are declared in the repo at `/workspace/.mcp.json`
(host: `<repo>/.mcp.json`), which is NOT baked into the image:

```json
{
  "mcpServers": {
    "web-search": {
      "command": "node",
      "args": ["/root/web-search-mcp/dist/index.js"],
      "env": { "BROWSER_HEADLESS": "true" }
    },
    "chromium": {
      "command": "npx",
      "args": ["-y", "chrome-devtools-mcp@1.6.0", "--headless", "--isolated",
               "--executablePath", "/usr/bin/chromium",
               "--chromeArg=--no-sandbox", "--chromeArg=--disable-gpu",
               "--no-usage-statistics", "--allow-unrestricted-paths"]
    },
    "firefox": {
      "command": "npx",
      "args": ["-y", "@playwright/mcp@0.0.78", "--browser", "firefox",
               "--headless", "--isolated",
               "--allow-unrestricted-file-access",
               "--output-dir", "/workspace/.playwright-mcp"]
    }
  }
}
```

### Which config file pi actually reads (verified empirically)

The pi-mcp-adapter resolves the project `.mcp.json` as `<process.cwd()>/.mcp.json`
— literal cwd, **no upward directory walk** (see `config.ts` in the adapter).
Verified with `pi -p --model nvidia/nemotron-3-ultra-550b-a55b` calling `mcp({})`:

- cwd `/workspace`: `/workspace/.mcp.json` **is** discovered (all 3 servers listed).
- cwd `/workspace/.pi/crack/server` (how this project's pi agents are spawned):
  `/workspace/.mcp.json` is **NOT** discovered ("No MCP servers configured").

Therefore the live container additionally has a copy at
**`/root/.config/mcp/mcp.json`** (user-global shared config), which the adapter
reads regardless of cwd — with it, all 3 servers connect from
`/workspace/.pi/crack/server`. Discovery priority (adapter docs):
`~/.config/mcp/mcp.json` > `~/.pi/agent/mcp.json` > `.mcp.json` > `.pi/mcp.json`.

For a fresh container, `_docker/_cont_start.sh` recreates the global copy on
every boot (and warns if the web-search-mcp build is missing):

```bash
mkdir -p /root/.config/mcp && cp /workspace/.mcp.json /root/.config/mcp/mcp.json
```

### Adapter proxy tool name

The adapter exposes a single proxy tool named **`mcp`** (plus optionally
per-server "direct tools" if configured — not used here). Usage inside an agent:
`mcp({})` status, `mcp({ server: "chromium" })` list server tools,
`mcp({ search: "navigate" })` search, `mcp({ describe: "tool" })`,
`mcp({ tool: "chromium_navigate_page", args: "{\"url\":\"...\"}" })` call.
Tool names are prefixed `<server>_<tool>` and fuzzy-matched on hyphens/underscores.

### File-access flags (why screenshots to /workspace work)

Both browser servers confine file-writing tools (screenshots, PDF, traces) to
their **workspace roots**. A server learns those roots by calling `roots/list`
on the MCP *client* — but pi-mcp-adapter never negotiates the `roots`
capability (`buildClientCapabilities()` only ever emits `sampling`/
`elicitation`), so neither server learns any root. chrome-devtools-mcp then
falls back to *temp-dir-only* and @playwright/mcp to *cwd-only*
(`/workspace/.pi/crack/server`), and any write to `/workspace/...` or `/root/...`
is rejected with `Access denied: … is not within any of the configured
workspace roots`.

Because the adapter leaves the servers' roots **undefined**, the per-server
escape hatch applies cleanly:

- **chromium** → `--allow-unrestricted-paths` (skips path validation when the
  client never negotiated roots).
- **firefox** → `--allow-unrestricted-file-access` (same effect for playwright),
  plus `--output-dir /workspace/.playwright-mcp` so its outputs land in the
  (git-ignored) workspace rather than a scratch temp dir.

Verified by driving chrome-devtools-mcp over raw JSON-RPC with a client that
advertises no `roots`: `take_screenshot { filePath: "/workspace/test_shot.png" }`
returned `Saved screenshot to /workspace/test_shot.png` and wrote a valid PNG.

### Network-reachable MCP endpoints (host access)

The stdio `.mcp.json` is only usable by the in-container `pi` agents. To let a
host user reach the *same* MCP servers over `localhost`, `_cont_start.sh` also
serves each one over HTTP (ports published in `run.sh`):

| Server | Host URL | Transport |
|--------|----------|-----------|
| firefox (playwright) | `http://localhost:9930/mcp` | Streamable HTTP (native `--host 0.0.0.0 --port`) |
| chromium (chrome-devtools) | `http://localhost:9931/sse` | SSE via `supergateway` |
| web-search | `http://localhost:9932/sse` | SSE via `supergateway` |

Notes / gotchas discovered while wiring this:
- **playwright/mcp** serves HTTP natively but its default `--allowed-hosts`
  rejects the Docker-proxied `Host` header with **403**; pass `--allowed-hosts "*"`.
- **chrome-devtools-mcp** and **web-search** are stdio-only, so `supergateway`
  bridges them to SSE. supergateway calls `listen(port)` with no host and binds
  IPv6 `::` only, which Docker's IPv4 userland proxy cannot reach. So each
  supergateway runs on an internal loopback port (`+10000`) and is exposed on
  `0.0.0.0` by `_docker/tcp_forward.py` (a tiny stdlib asyncio TCP forwarder,
  `TCP_NODELAY` set so the first SSE event isn't held by Nagle).
- Everything runs under a `respawn` loop in `_cont_start.sh`, so a crashed
  bridge self-heals. Logs: `/workspace/.pi/crack/harness/mcp-http/*.log`.

Host client config example (points at the container over localhost):

```json
{
  "mcpServers": {
    "firefox":    { "url": "http://localhost:9930/mcp" },
    "chromium":   { "url": "http://localhost:9931/sse" },
    "web-search": { "url": "http://localhost:9932/sse" }
  }
}
```

## Browser MCP server choices

- **chromium: `chrome-devtools-mcp@1.6.0`** (official Google, CDP/puppeteer).
  Chosen because it verifiably drives the *system* chromium 150 via
  `--executablePath /usr/bin/chromium`. Running as root requires
  `--chromeArg=--no-sandbox` (verified: navigate + `document.title` on
  https://example.com returned "Example Domain").
- **firefox: `@playwright/mcp@0.0.78 --browser firefox`** (Microsoft).
  Playwright cannot drive the system firefox-esr (it needs its own patched
  build), so it uses the playwright firefox 152.0.4 installed by
  `npx @playwright/mcp@0.0.78 install-browser firefox` (verified: example.com
  title "Example Domain"). The system firefox-esr 140.12 + geckodriver 0.37.0
  are installed and work via raw WebDriver (verified headless session
  creation), but no geckodriver-based MCP server was needed since
  playwright-mcp works.

## Verification results (all run live in the container)

- `chromium --version`, `chromedriver --version`, `firefox --version`,
  `geckodriver --version`, `wasm-pack --version` — all OK (versions above).
- web-search-mcp: MCP `initialize` + `tools/list` over stdio OK; exposes
  `full-web-search`, `get-web-search-summaries`, `get-single-web-page-content`.
- geckodriver + firefox-esr raw WebDriver headless session OK.
- End-to-end via `pi -p --model nvidia/nemotron-3-ultra-550b-a55b` (cwd
  `/workspace/.pi/crack/server`, global mcp config):
  - chromium MCP: opened https://example.com, title = `Example Domain`. OK
  - firefox MCP: opened https://example.com, title = `Example Domain`. OK

### 3-way search test: "weather in Las Vegas USA"

| Method | Result |
|--------|--------|
| (a) web-search-mcp `full-web-search` | REAL content — current conditions (~87-88°F, sunny, humidity ~46%) from Weather Network / BBC / NWS, some ad junk mixed in |
| (b) chromium MCP search | REAL content — 29°C/85°F sunny, high 44°C; no CAPTCHA/block (model reported pulling the data via wttr.in rather than the instructed DuckDuckGo page) |
| (c) firefox MCP DuckDuckGo query | REAL content — DDG instant-answer forecast verbatim (42°/30°C highs/lows). First attempt's prose answer was contaminated (model rambled about the project's local server and quoted a wrong "32°F"); a retry quoting `document.body.innerText` verbatim confirmed genuine DDG weather content |

All three methods return real weather content; none were hard-blocked. The
nvidia/nemotron-3-ultra model is prone to context contamination in prose
answers — prefer verbatim-quoting prompts when accuracy matters.

## Caveats

- **`/root` is a Docker volume** (`VOLUME /root` in the Dockerfile). Anything
  installed under `/root` in the image (web-search-mcp clone, playwright
  browser caches, `~/.config/mcp/mcp.json`, pi extensions) only reaches *new*
  containers (volume initialized from image content at creation). The existing
  `crack-dev` container keeps its current `/root` volume — image rebuilds will
  NOT update it; re-run the install commands inside the live container instead.
- **stdio MCP servers are not daemons**: the pi-mcp-adapter launches them
  lazily per session (first use) and kills them afterwards. `npx -y` servers
  (chromium, firefox) are cached by npm after first run.
- **root needs `--no-sandbox` for chromium** — already in the `.mcp.json` args.
- playwright's firefox (152.0.4) is *not* the system firefox-esr (140.12); the
  `firefox` MCP server drives the playwright build. Use geckodriver/WebDriver
  directly if you specifically need firefox-esr.
- web-search-mcp env knobs (see its source): `BROWSER_HEADLESS`,
  `BROWSER_TYPES` (default `chromium,firefox`), `MAX_BROWSERS`,
  `MAX_CONTENT_LENGTH`, `DEFAULT_TIMEOUT`, `FORCE_MULTI_ENGINE_SEARCH`.
- `chrome-devtools-mcp` restricts file-writing tools to the OS temp dir unless
  the client negotiates MCP roots (harmless warning on stderr).
