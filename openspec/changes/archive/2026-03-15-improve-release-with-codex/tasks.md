## 1. Extend release preflight coverage

- [x] 1.1 Update `scripts/release-preflight.mjs` to validate the expected `bump-version-<version>` branch and report current-branch readiness in its machine-readable output.
- [x] 1.2 Expand the preflight scan to surface release-sensitive non-manifest files and generated-document follow-ups, including `crates/typlite/README.md`, the Neovim Docker/bootstrap files, and `node scripts/link-docs.mjs`.
- [x] 1.3 Correct the regular/release-candidate release-notes handoff so the generated notes command uses the documented previous stable tag instead of the current workspace version.
- [x] 1.4 Codex repeatly invoke preflight for execution and checks. When checking passed, inform a report to the user. Add a changelog-summary report that identifies candidate release-note items represented in the changelog and items omitted for maintainer review.

## 2. Refine the Codex release workflow

- [x] 2.1 Update `.codex/skills/tinymist-release/SKILL.md` so the maintainer workflow stays in the Codex console and explicitly includes changelog-summary review, generated-doc refresh, the local bump commit, and the `yarn release <version>` approval boundary.
- [x] 2.2 Update `docs/tinymist/release-instruction.typ` so the human-readable release instructions match the refined Codex-assisted release sequence and the new preflight coverage.

## 3. Validate the refined release path

- [x] 3.1 Dry-run the updated preflight helper for a representative release-candidate target and verify the new readiness fields, changelog summary, and handoff commands.
- [x] 3.2 Review the resulting workflow guidance for consistency across the helper output, skill, and maintainer docs, and fix any drift before closing the change.
