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
RUN   git config --global user.email "127537716+johnny-smitherson@users.noreply.github.com"
RUN  git config --global user.name "johnny clankerson"

# Docker-out-of-docker: podman remote client. crack-dev drives the HOST podman
# (rootless) over a mounted socket (see run.sh: CONTAINER_HOST) to create the
# per-conversation sandbox containers as host-level SIBLINGS. Only the remote
# client is used here — no local storage/daemon runs inside crack-dev.
RUN apt-get update && apt-get install -y --no-install-recommends podman \
    && rm -rf /var/lib/apt/lists/*

ENV CRACK_HARNESS_DATA_DIR=/crack-harness-data

VOLUME /root
WORKDIR /workspace

EXPOSE 22 9876 9877

CMD ["/usr/sbin/sshd", "-D"]