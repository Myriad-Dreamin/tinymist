# ```shell
# docker build -t myriaddreamin/tinymist:latest .
# ```
#
# ## References
#
# https://stackoverflow.com/questions/58473606/cache-rust-dependencies-with-docker-build
# https://stackoverflow.com/a/64528456
# https://depot.dev/blog/rust-dockerfile-best-practices

ARG RUST_VERSION=1.89.0

FROM rust:${RUST_VERSION}-bookworm AS base
RUN apt-get install -y git
RUN cargo install sccache --version ^0.7
RUN cargo install cargo-chef --version ^0.1
ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache
# to download the toolchain
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    rustup update 

FROM base as planner
WORKDIR app
# We only pay the installation cost once, 
# it will be cached from the second build onwards
RUN cargo install cargo-chef 
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo +${RUST_VERSION} chef prepare --recipe-path recipe.json

FROM base as builder
WORKDIR app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo +${RUST_VERSION} chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo +${RUST_VERSION} build --bin tinymist --release

FROM debian:12
WORKDIR /app/
COPY --from=builder /app/target/release/tinymist /usr/local/bin
ENTRYPOINT ["/usr/local/bin/tinymist"]
