## Context

The current Codex-assisted release path already separates inspection, local preparation, and external actions, but the machine-readable preflight still behaves like a narrow manifest scanner. It does not validate the expected `bump-version-<version>` branch, does not surface release-sensitive non-manifest files, does not explain whether generated documentation needs to be refreshed, and does not summarize how the changelog differs from candidate release notes.

The maintainer workflow also needs a clearer handoff. Recent release practice expects the publisher to stay in the Codex console, review changelog coverage, refresh generated docs when needed, create a local `build: bump version to <version>` commit, and only then approve `yarn release <version>`. The current skill and helper describe parts of this flow, but they do not enforce or present it consistently.

This is a cross-cutting refinement because it touches the release helper, the Codex skill, and the maintainer release instructions together. If those three drift apart, Codex will again reconstruct the workflow from partial information.

## Goals / Non-Goals

**Goals:**
- Expand the release preflight into a fuller readiness report that includes branch validation, release-sensitive files, generated-doc follow-ups, changelog coverage, and a normalized next-step sequence.
- Keep the release helper as the primary machine-readable source so the skill can reuse repository logic instead of inferring release policy ad hoc.
- Make the local-preparation path explicit: changelog review, generated-doc refresh, review commands, local release commit, then approval request for `yarn release <version>`.
- Align helper output with the documented release-note comparison rules for release candidates and stable releases.

**Non-Goals:**
- Replacing `scripts/release.mjs` or the GitHub Actions release pipeline.
- Fully automating changelog editing or release-note curation without maintainer review.
- Requiring a new external service or complex parser for release-note classification if simpler repository-local heuristics are sufficient.
- Redefining nightly release semantics beyond the shared preflight and handoff improvements.

## Decisions

### 1. Extend preflight with structured readiness checks instead of prompt-only rules

`scripts/release-preflight.mjs` should grow from manifest patch generation into a structured readiness helper that reports:
- expected branch name and current branch status
- release-sensitive files beyond manifests
- generated-doc follow-up commands such as `node scripts/link-docs.mjs`
- changelog coverage findings
- normalized review and handoff commands

This keeps the skill grounded in repository-owned logic and makes dry runs repeatable.

Alternative considered:
- Keep the extra checks only in the skill text. Rejected because the logic would drift from scripts and would be harder to validate during real releases.

### 2. Treat generated documentation as a first-class release follow-up

Some release-sensitive outputs are generated from Typst sources rather than edited directly. For example, `crates/typlite/README.md` is produced by `node scripts/link-docs.mjs`. Preflight should therefore identify generated outputs that may drift with a version bump and surface the source command to refresh them, rather than suggesting manual edits to generated markdown files.

Alternative considered:
- Patch generated markdown files directly in the release helper. Rejected because it violates repository conventions and would fight the existing doc generation workflow.

### 3. Add an advisory changelog-coverage summary instead of full changelog automation

The helper should compare candidate release-note items against the current changelog entry and report two buckets:
- items represented in the changelog
- items omitted from the changelog and left for maintainer review

This summary is advisory, not authoritative. The maintainer still owns the final changelog content, but Codex should no longer hand off changelog editing without explaining what appears covered and what still needs judgment.

Alternative considered:
- Fully auto-classify and rewrite the changelog entry. Rejected because the release instructions already describe nuanced editorial rules that still require publisher judgment.

### 4. Make the local release commit an explicit workflow step before external approval

The skill and docs should present a consistent local-preparation sequence: apply or review local edits, refresh generated docs if needed, inspect the diff, and create a local commit with `build: bump version to <version>`. Only after that reviewable checkpoint should Codex ask for approval to run `yarn release <version>`.

Alternative considered:
- Treat the commit as implicit or let `yarn release` handle all remaining version-related changes. Rejected because the maintainer wants a stable, reviewable checkpoint before the first externally visible release action.

### 5. Normalize regular/RC release-note generation to the documented previous stable tag

For release candidates and stable releases, the helper should generate the release-notes command using the previous stable tag rather than blindly reusing the current workspace version. This aligns the helper output with the documented changelog workflow and avoids confusing dry-run guidance.

Alternative considered:
- Leave the helper as-is and document the correction in prose. Rejected because the helper output is supposed to be the primary handoff artifact for Codex.

### 6. Use GitHub-generated notes instead of local git history for changelog coverage when possible

When GitHub tooling is available, the helper should derive candidate release-note items from the same source as the changelog instructions: GitHub-generated notes based on merged PRs. This ensures the helper's changelog summary reflects the same candidate pool that the maintainer will see when following the release instructions.

## Risks / Trade-offs

- [Preflight grows more complex] -> Mitigate by keeping checks focused on repository-local signals and emitting structured output instead of narrative-only summaries.
- [Changelog coverage heuristics may miss editorial nuance] -> Mitigate by treating the coverage summary as advisory and explicitly preserving maintainer review.
- [Skill and docs can drift from helper behavior] -> Mitigate by updating `SKILL.md`, `release-preflight.mjs`, and `release-instruction.typ` in the same change.
- [Local commit semantics may overlap with later release-script commits] -> Mitigate by documenting the intended checkpoint clearly and verifying whether subsequent scripts still create additional commits.

## Migration Plan

1. Extend `scripts/release-preflight.mjs` to report the new readiness fields and corrected handoff commands.
2. Update the `tinymist-release` skill so the maintainer-facing sequence matches the new helper output, including the changelog-summary and local commit steps.
3. Update `docs/tinymist/release-instruction.typ` to keep the human-readable release policy aligned with the refined Codex path.
4. Dry-run the helper and workflow against a representative release-candidate target to confirm the new readiness summary and handoff order.

## Open Questions

- Should the release helper merely report the expected `build: bump version to <version>` commit command, or also generate the exact `git add` and `git commit` commands for the skill to reuse?
- Does the later `scripts/release.mjs` behavior need follow-up refinement so its own asset-update commit remains compatible with the new pre-release checkpoint?
