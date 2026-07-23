FROM docker.io/johnnysmitherson/crack-dev:base

# Install Blender 5.1.2 dependencies + xvfb for headless GUI
RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    imagemagick \
    xvfb \
    libgl1 \
    libxi6 \
    libxrender1 \
    libxxf86vm1 \
    && rm -rf /var/lib/apt/lists/*

# Download and install Blender 5.1.2
ARG BLENDER_VERSION=5.1.2
ARG BLENDER_SHA256=aaccb355f50183979b698bcce7467103a76261b5fa59f4972295842662a285fb
ARG BLENDER_URL=https://download.blender.org/release/Blender5.1/blender-5.1.2-linux-x64.tar.xz

RUN cd /opt \
    && wget -q "${BLENDER_URL}" -O blender.tar.xz \
    && echo "${BLENDER_SHA256}  blender.tar.xz" | sha256sum -c - \
    && tar -xf blender.tar.xz \
    && mv blender-${BLENDER_VERSION}-linux-x64 blender-${BLENDER_VERSION} \
    && ln -sf /opt/blender-${BLENDER_VERSION}/blender /usr/local/bin/blender \
    && rm blender.tar.xz

# Install blender-mcp (MCP server CLI; Blender addon is vendored separately)
# Use --break-system-packages since container is isolated
RUN pip install --no-cache-dir --break-system-packages blender-mcp==1.6.4

# Stable path for the addon (synced into /root on every boot — see _cont_start.sh)
COPY blender_mcp_addon.py /opt/blender_mcp_addon.py

RUN pi install npm:pi-antigravity
RUN pi install npm:pi-cursor-agent && pi uninstall npm:pi-cursor-sdk
RUN pi install npm:pi-readseek

# Patch pi-mcp-adapter (installed in the base image) so the `mcp` gateway tool
# accepts `args` as a native object, not only a double-encoded JSON string. pi
# validates tool args against the schema BEFORE the handler runs, so models that
# pass `args:{...}` directly were rejected with `args: must be string` even though
# the arguments were valid. Upstream v2.11.0 == commit
# 82724dccc13a49310530898f922bafff12b7f3fe; see the patch file for full rationale.
# NOTE: /root is a runtime volume (see VOLUME below); this bakes the fix into the
# image so freshly-created volumes inherit it. An existing crack-dev-root-dir
# volume shadows /root and must be patched out-of-band.
COPY patches/pi-mcp-adapter-args-object.patch /tmp/pi-mcp-adapter-args-object.patch
RUN patch -p1 -d /root/.pi/agent/npm/node_modules/pi-mcp-adapter \
        < /tmp/pi-mcp-adapter-args-object.patch \
    && grep -q 'Type.Union' /root/.pi/agent/npm/node_modules/pi-mcp-adapter/index.ts \
    && rm -f /tmp/pi-mcp-adapter-args-object.patch
RUN   git config --global user.email "127537716+johnny-smitherson@users.noreply.github.com"
RUN  git config --global user.name "johnny clankerson"

# Docker-out-of-docker: podman remote client. crack-dev drives the HOST podman
# (rootless) over a mounted socket (see run.sh: CONTAINER_HOST) to create the
# per-conversation sandbox containers as host-level SIBLINGS. Only the remote
# client is used here — no local storage/daemon runs inside crack-dev.
RUN apt-get update && apt-get install -y --no-install-recommends podman \
    && rm -rf /var/lib/apt/lists/*

ENV CRACK_HARNESS_DATA_DIR=/crack-harness-data

# The crack-pi-server package is managed with Poetry (uv can't relocate its
# per-project .venv out of the source tree, which the frozen git-tree sandbox
# base must exclude). Point ALL Poetry virtualenvs at the existing target volume
# (crack-dev-target-dir, mounted at /workspace/target, gitignored) so the venv
# lives in the one build/cache volume and never becomes a sandbox overlay lower.
# No extra volume is created. The venv is built at boot by `poetry install`
# (see _cont_start.sh) against the bind-mounted source.
RUN pip install --no-cache-dir --break-system-packages poetry
ENV POETRY_VIRTUALENVS_PATH=/workspace/target/python-venvs/

# Poetry 2.x and virtualenv shell out to a bare `python`, but Debian ships only
# `python3`/`python3.13` and the base image's `/usr/local/python/bin` (from
# UV_PYTHON_INSTALL_DIR) is never populated (no `uv python install` step). Without
# this symlink `poetry install` dies with FileNotFoundError: 'python' and the
# container crash-loops. With it, Poetry resolves the interpreter and honors
# POETRY_VIRTUALENVS_PATH above (venv in the overlaid target volume, not in-project).
RUN ln -sf /usr/bin/python3 /usr/local/bin/python

VOLUME /root
WORKDIR /workspace

EXPOSE 22 9876 9877

CMD ["/usr/sbin/sshd", "-D"]