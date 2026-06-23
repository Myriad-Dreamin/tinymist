#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

image_name="${TINYMIST_NVIM_IMAGE:-tinymist-nvim-spec-local}"

case "${1:-}" in
  test)
    shift
    docker_args=(python3 ./spec/main.py "$@")
    ;;
  bash)
    docker_args=(bash)
    ;;
  editor)
    docker_args=(nvim .)
    ;;
  *)
    echo "Usage: $0 [test|bash|editor]"
    exit 1
    ;;
esac

if [ -n "${TINYMIST_BIN:-}" ]; then
  tinymist_bin="$TINYMIST_BIN"
elif [ -x "$repo_root/target/debug/tinymist" ]; then
  tinymist_bin="$repo_root/target/debug/tinymist"
elif [ -x "$repo_root/target/release/tinymist" ]; then
  tinymist_bin="$repo_root/target/release/tinymist"
else
  echo "No tinymist binary found."
  echo "Build one first with: cargo build --bin tinymist"
  echo "Or set TINYMIST_BIN=/absolute/path/to/tinymist."
  exit 1
fi

if [ ! -x "$tinymist_bin" ]; then
  echo "Tinymist binary is not executable: $tinymist_bin"
  exit 1
fi

mkdir -p "$script_dir/target/.local" "$script_dir/target/.cache"

docker_build_args=(-t "$image_name" -f lazyvim-dev/Dockerfile)
if [ -n "${TINYMIST_NVIM_DOCKER_BUILD_NETWORK:-}" ]; then
  docker_build_args+=(--network "$TINYMIST_NVIM_DOCKER_BUILD_NETWORK")
fi

(cd "$script_dir/samples" && docker build "${docker_build_args[@]}" .)

docker_run_args=(
  --rm
  --tmpfs /home/runner/dev/workspaces:uid=1000,gid=1000,mode=755
  --tmpfs /home/runner/.local:uid=1000,gid=1000,mode=755
  --tmpfs /home/runner/.cache:uid=1000,gid=1000,mode=755
  -v "$repo_root/tests/workspaces:/home/runner/workspaces-src:ro"
  -v "$script_dir:/home/runner/dev"
  -v "$tinymist_bin:/usr/local/bin/tinymist:ro"
  -w /home/runner/dev
)

if [ -t 0 ] && [ -t 1 ]; then
  docker_run_args+=(-it)
fi

docker run "${docker_run_args[@]}" "$image_name" bash -lc '
  set -euo pipefail
  cp -R /home/runner/workspaces-src/book /home/runner/dev/workspaces/book
  exec "$@"
' bash "${docker_args[@]}"
