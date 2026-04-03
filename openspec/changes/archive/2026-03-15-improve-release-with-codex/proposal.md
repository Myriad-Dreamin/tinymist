## Why

The initial Codex-assisted release workflow established an inspect-first release path, but recent dry-run experience showed several important gaps in the repository preflight and maintainer handoff. Those gaps make it too easy for Codex to miss release-sensitive files, under-report changelog coverage, or proceed without surfacing the exact local preparation sequence maintainers now expect.

## What Changes

- Tighten the Codex-assisted release preflight so it verifies release readiness against the expected `bump-version-<version>` branch naming convention.
- Expand preflight reporting beyond manifests to include release-sensitive files and generated-document follow-ups such as `crates/typlite/README.md`, the Neovim Docker and bootstrap files, and `node scripts/link-docs.mjs`.
- Add a changelog-summary step that tells the publisher what release-note items are represented in the changelog and which candidate items are omitted for manual review.
- Refine the local-preparation workflow so Codex explicitly guides the maintainer through the release commit step using `build: bump version to <version>`.
- Clarify the external approval boundary so Codex asks immediately before `yarn release <version>` and presents that command as the next irreversible handoff.
- Align the release helper output with the documented release-notes conventions used for release candidates and stable releases.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `codex-assisted-release`: strengthen preflight coverage, changelog review reporting, and the guided handoff sequence for local commit and external release execution.

## Impact

- `scripts/release-preflight.mjs` and related helper logic for machine-readable release inspection
- `.codex/skills/tinymist-release/` for the maintainer-facing Codex workflow
- `docs/tinymist/release-instruction.typ` as the human-readable release policy source
- Release-sensitive repository files and generated-doc workflows that should be surfaced during preflight review
