#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

default_input="target/preview-touying-0.6.0.typ"
default_bundle_input="target/preview-touying-0.6.0.bundle/index.typ"
mode="${1:-watch}"
if [[ "$mode" == "bundle" || "$mode" == "bundle-once" || "$mode" == "watch-bundle" || "$mode" == "serve-bundle" ]]; then
  input="${LSIF_INPUT:-$default_bundle_input}"
  output="${LSIF_OUTPUT:-target/test-bundle}"
else
  input="${LSIF_INPUT:-$default_input}"
  output="${LSIF_OUTPUT:-target/test.html}"
fi
x_target="${LSIF_X_TARGET:-html-ayu}"

usage() {
  cat <<'EOF'
Usage: scripts/dev-lsif.sh [once|watch|serve|bundle|watch-bundle|serve-bundle|build-index]

Modes:
  once          Compile the generated package-doc Typst file once.
  watch         Watch and recompile without starting Typst's HTML TCP server.
  serve         Watch and recompile with Typst's HTML server enabled.
  bundle        Compile the generated bundle package-doc Typst entry once.
  watch-bundle  Watch and recompile the bundle entry without Typst's HTML TCP server.
  serve-bundle  Watch and recompile the bundle entry with Typst's HTML server enabled.
  build-index   Build typ/packages/tinymist-index/tinymist_index.wasm.

Environment:
  LSIF_INPUT     Typst input file. Defaults to target/preview-touying-0.6.0.typ
                 or target/preview-touying-0.6.0.bundle/index.typ in bundle modes.
  LSIF_OUTPUT    HTML output file or bundle output directory. Defaults to target/test.html
                 or target/test-bundle in bundle modes.
  LSIF_X_TARGET  Typst x-target input. Defaults to html-ayu.
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing required command: $1" >&2
    exit 127
  fi
}

prepare_bundle_output() {
  case "$output" in
    ""|"/"|".")
      echo "error: refusing to clean unsafe bundle output path: $output" >&2
      exit 2
      ;;
  esac

  rm -rf -- "$output"
  mkdir -p "$(dirname -- "$output")"
}

ensure_input() {
  local lsif="target/preview-touying-0.6.0.lsif.jsonl"
  if [[ -f "$input" && ( "$input" != "$default_input" || -f "$lsif" ) && ( "$input" != "$default_bundle_input" || -f "$lsif" ) ]]; then
    return
  fi

  if [[ "$input" == "$default_input" || "$input" == "$default_bundle_input" ]]; then
    echo "Generating $input and $lsif with the package-doc touying fixture..."
    require_cmd cargo
    cargo test -p tinymist-query docs::package::tests::touying -- --nocapture
  fi

  if [[ ! -f "$input" ]]; then
    echo "error: input file does not exist: $input" >&2
    echo "hint: set LSIF_INPUT or generate package docs first." >&2
    exit 1
  fi
}

typst_args=(
  --root .
  --font-path assets/fonts
  --input "x-target=$x_target"
  "$input"
  "$output"
)

case "$mode" in
  once|compile)
    require_cmd typst
    ensure_input
    mkdir -p "$(dirname -- "$output")"
    typst compile --features html "${typst_args[@]}"
    ;;
  bundle|bundle-once)
    require_cmd typst
    ensure_input
    prepare_bundle_output
    typst compile --features bundle,html --format bundle "${typst_args[@]}"
    ;;
  watch)
    require_cmd typst
    ensure_input
    mkdir -p "$(dirname -- "$output")"
    typst watch --no-serve --features html "${typst_args[@]}"
    ;;
  watch-bundle)
    require_cmd typst
    ensure_input
    prepare_bundle_output
    typst watch --no-serve --features bundle,html --format bundle "${typst_args[@]}"
    ;;
  serve)
    require_cmd typst
    ensure_input
    mkdir -p "$(dirname -- "$output")"
    typst watch --features html "${typst_args[@]}"
    ;;
  serve-bundle)
    require_cmd typst
    ensure_input
    prepare_bundle_output
    typst watch --features bundle,html --format bundle "${typst_args[@]}"
    ;;
  build-index)
    require_cmd node
    node scripts/build.mjs build:index
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
