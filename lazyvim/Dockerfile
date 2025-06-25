FROM ubuntu:24.04 AS builder

RUN apt-get update && apt-get install -y \
    git \
    file \
    ninja-build gettext cmake unzip curl build-essential

RUN git clone --filter=blob:none --branch stable https://github.com/neovim/neovim && cd neovim && make CMAKE_BUILD_TYPE=RelWithDebInfo
RUN cd neovim/build && cpack -G DEB && dpkg -i nvim-linux64.deb


FROM ubuntu:24.04

COPY --from=builder /neovim/build/nvim-linux64.deb /tmp/nvim-linux64.deb
RUN apt-get update && apt-get install -y \
    /tmp/nvim-linux64.deb \
    curl \
    gcc \
    git \
    make \
    ripgrep \
    zsh \
    && rm /tmp/nvim-linux64.deb

RUN useradd --create-home --shell /bin/zsh lean
USER lean
WORKDIR /home/lean

RUN curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh -s -- -y --default-toolchain none

ENV PATH="/home/lean/.elan/bin:${PATH}"

RUN git clone --filter=blob:none https://github.com/LazyVim/starter ~/.config/nvim
COPY lean.lua .config/nvim/lua/plugins/lean.lua

ARG LEAN_PROJECT=https://github.com/leanprover-community/mathlib4

RUN git clone --filter=blob:none $LEAN_PROJECT && cd $(basename "$LEAN_PROJECT") && lake exe cache get && elan default "$(cat lean-toolchain || echo stable)"

# SHELL isn't supported by OCI images
CMD ["zsh", "-l"]
