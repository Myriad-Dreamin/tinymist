## Why

Tinymist releases currently depend on maintainers manually stitching together `docs/tinymist/release-instruction.typ`, release scripts, changelog updates, asset publishing, and tag-based CI behavior. That works, but it makes Codex-assisted release work ad hoc and easy to get wrong because the safety rules live in multiple places and some steps are irreversible.

We need a Codex-native release workflow that can inspect repository state, prepare the reversible parts of a release, and clearly stop before external side effects unless a maintainer explicitly approves them.

## What Changes

- Add a Codex-facing release workflow that guides maintainers through release preparation using the repo's existing release documentation, scripts, and workflow conventions.
- Teach the workflow to validate release prerequisites such as version consistency, changelog presence, release-candidate expectations, and known release checklist items before making edits.
- Support preparation tasks that are safe to automate locally, such as summarizing release readiness, drafting or updating versioned files, and composing release notes or PR metadata.
- Separate reversible repository edits from irreversible actions such as publishing crates, creating tags, or mutating GitHub releases, and require an explicit maintainer confirmation boundary before any external side effect.
- Document how maintainers invoke the Codex workflow and how it relates to `scripts/release.mjs`, `scripts/draft-release.mjs`, and the existing CI release path.

## Capabilities

### New Capabilities
- `codex-assisted-release`: Allow Codex to prepare a tinymist release by reading repository state, validating release preconditions, updating release artifacts, and handing off externally visible publish steps for explicit maintainer approval.

### Modified Capabilities
- None.

## Impact

- `.github/skills/` and `.github/prompts/` for the new Codex release workflow entry point
- `docs/tinymist/release-instruction.typ` and related maintainer-facing documentation
- Potential helper automation in `scripts/` for machine-checkable release validation and release-note preparation
- Existing release scripts and GitHub workflows as integration points, without replacing the current release pipeline
