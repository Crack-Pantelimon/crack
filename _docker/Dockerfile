FROM debian:13-slim

ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
ENV UV_PYTHON_INSTALL_DIR=/usr/local/python
ENV PATH=/usr/local/python/bin:$PATH
ENV NODE_VERSION=22

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    wget \
    gnupg \
    lsb-release \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    libx11-dev \
    libasound2-dev \
    libudev-dev \
    libxkbcommon-x11-0 \
    libwayland-dev \
    libxkbcommon-dev \
    libvulkan-dev \
    vulkan-tools \
    mesa-vulkan-drivers \
    vulkan-validationlayers \
    htop \
    tmux \
    vim \
    ripgrep \
    fzf \
    bat \
    eza \
    fd-find \
    zoxide \
    jq \
    openssh-server \
    python3 \
    python3-pip \
    python3-venv \
     lld mold clang \
    && rm -rf /var/lib/apt/lists/*

RUN (command -v bat >/dev/null || ln -sf /usr/bin/batcat /usr/local/bin/bat) ; \
    (command -v fd  >/dev/null || ln -sf /usr/bin/fdfind /usr/local/bin/fd) ; true

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal \
    && rustup component add rustfmt clippy \
    && cargo install -f cargo-binutils trunk wasm-pack \
    && rustup component add llvm-tools-preview \
    && rustup target add wasm32-unknown-unknown

RUN curl -LsSf https://astral.sh/uv/install.sh | sh \
    && mv /root/.local/bin/uv /usr/local/bin/uv \
    && mv /root/.local/bin/uvx /usr/local/bin/uvx

RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get update && apt-get install -y --no-install-recommends nodejs \
    && npm install -g npm@latest \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /var/run/sshd \
    && sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config \
    && echo 'root:root' | chpasswd \
    && ssh-keygen -A

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV RUSTUP_TOOLCHAIN=stable
ENV TRUNK_SKIP_VERSION_CHECK=1
ENV TELEMETRY=0


RUN curl -fsSL https://pi.dev/install.sh | sh

RUN echo "set -g extended-keys on" > /root/.tmux.conf
RUN echo "set -g extended-keys-format csi-u" >> /root/.tmux.conf
RUN npm install -g sigmap
RUN pi install npm:pi-cursor-sdk
RUN pi install npm:pi-mcp-adapter

# Browsers + WebDriver (chromedriver comes with chromium-driver;
# geckodriver has no Debian package -> pinned upstream tarball)
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    chromium-driver \
    firefox-esr \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sL -o /tmp/geckodriver.tar.gz https://github.com/mozilla/geckodriver/releases/download/v0.37.0/geckodriver-v0.37.0-linux64.tar.gz \
    && tar -xzf /tmp/geckodriver.tar.gz -C /tmp \
    && install -m 0755 /tmp/geckodriver /usr/local/bin/ \
    && rm /tmp/geckodriver.tar.gz /tmp/geckodriver

# web-search-mcp: stdio MCP server (playwright + axios fallback), pinned commit
RUN git clone https://github.com/mrkrsl/web-search-mcp /root/web-search-mcp \
    && cd /root/web-search-mcp \
    && git checkout eeb03f88525cbf74c4019e59a3fea45a537a760b \
    && npm install \
    && npx playwright install chromium firefox \
    && npm run build

# playwright-mcp needs its own patched firefox build (cannot drive system firefox-esr)
RUN npx -y @playwright/mcp@0.0.78 install-browser firefox

env CRACK_PI_PROJECT_ROOT=/workspace

VOLUME /root
WORKDIR /workspace


EXPOSE 22

CMD ["/usr/sbin/sshd", "-D"]