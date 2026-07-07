## Why

Tinymist already contains the building blocks for small Rust-based Typst tools, but the guidance is scattered across crates, internal docs, and examples such as `crityp` and `tinymist-cli`. That makes it hard to reuse the right API layer in downstream repositories, and it especially hurts the goal of shipping a single portable `SKILL.md` that can be copied elsewhere without bringing the whole repo along.

## What Changes

- Add a generated, standalone Codex skill that teaches how to build Rust-based Typst tools on top of Tinymist and Typst.
- Make the skill's source of truth live in Typst documents under `docs/tinymist/dev`, not in hand-maintained Markdown.
- Split the source content into two major sections:
  - how to make a tool, including dependency pinning, repository import patterns, and concrete examples
  - how to use Tinymist Rust APIs, including layered API guidance plus explicit do and do-not examples
- Assemble the API guidance from a `docs/tinymist` subdirectory into a single generated skill section with `#include` and `#import`.
- Cover the requested tool scenarios with concrete guidance and examples:
  - word count
  - compile in parallel
  - watch and query
- Reuse real repository patterns from `crates/crityp` and `crates/tinymist-cli` so the skill reflects maintained practice instead of synthetic examples.
- Extend the documentation generation workflow so the standalone skill file is regenerated from Typst sources instead of edited manually.

## Capabilities

### New Capabilities
- `tinymist-rust-tool-skill`: Provide a generated standalone skill and supporting Typst source docs for authoring Rust-based Typst tools with Tinymist APIs.

### Modified Capabilities
- None.

## Impact

- `docs/tinymist/dev/` for the new Typst source documents
- `.codex/skills/` for the generated standalone skill output
- `scripts/link-docs.mjs` and related docs generation wiring
- Existing example/reference crates including `crates/crityp` and `crates/tinymist-cli`
