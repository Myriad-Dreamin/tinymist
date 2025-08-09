# Not for end users! This is a Dockerfile to develop the Neovim plugin it self.
# https://github.com/Julian/lean.nvim/tree/1b2752069d700a3e6c7953f5c117d49c134ec711/.devcontainer/lazyvim

FROM debian:12 AS builder

RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get update && apt-get install -y \
    git \
    file \
    ninja-build gettext cmake unzip curl build-essential

RUN git clone --filter=blob:none --branch stable https://github.com/neovim/neovim && cd neovim && make CMAKE_BUILD_TYPE=RelWithDebInfo
USER root
RUN cd neovim/build && cpack -G DEB && dpkg -i nvim-linux-x86_64.deb

FROM myriaddreamin/tinymist:0.13.23 as tinymist

FROM debian:12

COPY --from=builder /neovim/build/nvim-linux-x86_64.deb /tmp/nvim-linux-x86_64.deb

RUN --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
    --mount=target=/var/cache/apt,type=cache,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean && \
    apt-get update && apt-get install -y curl git ripgrep build-essential unzip
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

RUN for dependency in AndrewRadev/switch.vim andymass/vim-matchup neovim/nvim-lspconfig nvim-lua/plenary.nvim tomtom/tcomment_vim lewis6991/satellite.nvim; do git clone --quiet --filter=blob:none "https://github.com/$dependency" "packpath/$(basename $dependency)"; done
RUN for dependency in Julian/inanis.nvim; do git clone --quiet --filter=blob:none "https://github.com/$dependency" "packpath/$(basename $dependency)"; done

USER root
COPY --from=tinymist /usr/local/bin/tinymist /usr/local/bin/tinymist
USER runner

ENV XDG_CONFIG_HOME=/home/runner/.config
ENV XDG_DATA_HOME=/home/runner/.local/share
ENV XDG_STATE_HOME=/home/runner/.local/state
ENV XDG_CACHE_HOME=/home/runner/.cache

COPY lazyvim-dev/init.lua $XDG_CONFIG_HOME/nvim/init.lua
COPY lazyvim-dev/plugins/dev.lua $XDG_CONFIG_HOME/nvim/lua/plugins/dev.lua
COPY lazyvim-dev/plugins/mason-workaround.lua $XDG_CONFIG_HOME/nvim/lua/plugins/mason-workaround.lua
COPY lazyvim/plugins/lsp-folding.lua $XDG_CONFIG_HOME/nvim/lua/plugins/lsp-folding.lua
COPY lazyvim/plugins/tinymist.lua $XDG_CONFIG_HOME/nvim/lua/others/tinymist.lua

# SHELL isn't supported by OCI images
ENTRYPOINT []

