#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

image_name="${TINYMIST_NVIM_IMAGE:-tinymist-nvim-spec-local}"

case "${1:-}" in
  test)
    docker_args=(python3 ./spec/main.py)
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

(cd ../.. && docker build -t myriaddreamin/tinymist:0.14.21-rc2 .)
(cd samples && docker build -t myriaddreamin/tinymist-nvim:0.14.21-rc2 -f lazyvim-dev/Dockerfile .)
docker run --rm -it \
  -v $PWD/../../tests/workspaces:/home/runner/dev/workspaces \
  -v $PWD:/home/runner/dev \
  -v $PWD/target/.local:/home/runner/.local \
  -v $PWD/target/.cache:/home/runner/.cache \
  -w /home/runner/dev myriaddreamin/tinymist-nvim:0.14.21-rc2 \
  $DOCKER_ARGS
