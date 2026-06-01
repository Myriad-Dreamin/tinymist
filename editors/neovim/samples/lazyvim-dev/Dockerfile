# Not for end users! This image develops and tests the Neovim plugin itself.
# It intentionally does not bundle tinymist. Mount the checkout's tinymist
# binary into /usr/local/bin/tinymist when running the container.

FROM debian:12

RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    wget \
    git \
    ripgrep \
    build-essential \
    unzip \
    python3 && \
    update-ca-certificates

RUN git clone --filter=blob:none --branch stable https://github.com/neovim/neovim && cd neovim && make CMAKE_BUILD_TYPE=RelWithDebInfo
USER root
RUN cd neovim/build && cpack -G DEB && dpkg -i nvim-linux-x86_64.deb

FROM myriaddreamin/tinymist:0.14.21-rc2 as tinymist

FROM debian:12

COPY --from=builder /neovim/build/nvim-linux-x86_64.deb /tmp/nvim-linux-x86_64.deb

RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get update && apt-get install -y curl wget git ripgrep build-essential unzip
RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get update && apt-get install -y python3
RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get install -y /tmp/nvim-linux-x86_64.deb \
    && rm /tmp/nvim-linux-x86_64.deb

RUN useradd --create-home --shell /bin/bash runner
USER runner
WORKDIR /home/runner

RUN set -eux; \
    for dependency in \
      AndrewRadev/switch.vim \
      andymass/vim-matchup \
      neovim/nvim-lspconfig \
      nvim-lua/plenary.nvim \
      tomtom/tcomment_vim \
      lewis6991/satellite.nvim \
      Julian/inanis.nvim; do \
        for attempt in 1 2 3; do \
          git clone --quiet --filter=blob:none "https://github.com/$dependency" "packpath/$(basename "$dependency")" && break; \
          if [ "$attempt" = 3 ]; then exit 1; fi; \
          sleep 2; \
        done; \
    done

ENV XDG_CONFIG_HOME=/home/runner/.config
ENV XDG_DATA_HOME=/home/runner/.local/share
ENV XDG_STATE_HOME=/home/runner/.local/state
ENV XDG_CACHE_HOME=/home/runner/.cache

COPY lazyvim-dev/init.lua $XDG_CONFIG_HOME/nvim/init.lua
COPY lazyvim-dev/plugins/dev.lua $XDG_CONFIG_HOME/nvim/lua/plugins/dev.lua
COPY lazyvim-dev/plugins/mason-workaround.lua $XDG_CONFIG_HOME/nvim/lua/plugins/mason-workaround.lua
COPY lazyvim/plugins/lsp-folding.lua $XDG_CONFIG_HOME/nvim/lua/plugins/lsp-folding.lua
COPY lazyvim/plugins/tinymist.lua $XDG_CONFIG_HOME/nvim/lua/others/tinymist.lua

# SHELL isn't supported by OCI images.
ENTRYPOINT []
