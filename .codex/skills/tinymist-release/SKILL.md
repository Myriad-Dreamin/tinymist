---
name: tinymist-release
description: For command `/tinymist-dev:release version`, prepare a tinymist release with an inspect-first workflow that reuses the repository's existing release scripts and stops before external side effects until the maintainer explicitly approves them.
license: Apache-2.0
compatibility: Requires Node.js, rg (ripgrep), git, a POSIX sh, the repository checkout, and optionally gh for GitHub handoff commands.
metadata:
  author: tinymist
  version: "1.0"
---

Prepare a tinymist release in three phases:

1. inspection and planning
2. local preparation
3. external actions

This workflow is intentionally conservative. It should help a maintainer inspect release readiness, prepare reviewable local edits, and then stop before any command that would mutate GitHub, publish crates, or push tags until the maintainer explicitly approves that exact action.

## Input

The user should provide a target version such as `v0.14.12-rc1`, `0.14.12`, or `0.14.11`.

If the version is missing, ask for the exact version before doing anything else.

## Workflow

1. Announce the release target and infer the release type

   - Strip any leading `v`
   - Classify the target as:
     - `release-candidate` when it ends with `-rcN`
     - `nightly` when the patch number is odd
     - `stable` when the patch number is even and there is no `-rcN`
   - Mention that stable releases should normally follow a successful release candidate, because that expectation is documented in `docs/tinymist/release-instruction.typ`

2. Run the repo-native release helper

   ```bash
   node scripts/release-preflight.mjs <target-version> --json
   ```

   Use the JSON output as the primary machine-readable source for:
   - branch readiness (`expectedBranch`, `branch.current`, `branch.ready`)
   - tracked `Cargo.toml` and `package.json` files
   - exact version-bearing lines that match the current release version
   - release-sensitive non-manifest files such as `editors/neovim/bootstrap.sh` and `editors/neovim/samples/lazyvim-dev/Dockerfile`
   - generated-document follow-up commands such as `node scripts/link-docs.mjs` for `crates/typlite/README.md`
   - changelog status plus any changelog-summary coverage the helper can compute from GitHub-generated notes
   - executable shell commands for update, review, prepare, re-check, and later handoff steps

3. Read the release policy and the relevant automation entry points

   Always read:
   - `docs/tinymist/release-instruction.typ`
   - `scripts/release.mjs`
   - `scripts/draft-release.mjs`

   When the target is nightly, also read:
   - `.github/workflows/release-nightly.yml`
   - `scripts/nightly-utils.mjs`

   When the target is a regular or release-candidate build, prefer the repository's existing release path:
   - `scripts/release.mjs`
   - `.github/workflows/release-asset-crate.yml`
   - `.github/workflows/auto-tag.yml`
   - `.github/workflows/announce.yml`

4. Report inspection results before making edits

   Summarize:
   - the inferred release type
   - the helper's readiness status, blockers, pending local-preparation items, and warnings
   - the expected `bump-version-<version>` branch and whether the current branch matches it and is pushed to remote.
   - the tracked manifest scan command
   - the exact update, review, prepare, re-check, and handoff commands the helper generated
   - the changelog summary: which candidate release-note items are represented in `editors/vscode/CHANGELOG.md`, which are omitted for maintainer review, or why the helper could not compute that summary
   - which exact external commands or workflows would come later

5. Local preparation is allowed without a separate approval prompt

   Reversible, reviewable checkout-local work is allowed, for example:
   - running the generated patch commands
   - drafting or editing `editors/vscode/CHANGELOG.md`
   - refreshing generated docs with the helper's follow-up commands such as `node scripts/link-docs.mjs`
   - composing the release PR title/body
   - running the generated review commands
   - staging the local release edits and creating `build: bump version to <version>`

   After local preparation:
   - rerun `node scripts/release-preflight.mjs <target-version> --json`
   - summarize which generated commands were used
   - if the helper still reports blockers or pending local-preparation items, stop and report them instead of moving on
   - if the helper reports `readiness.ready: true`, explicitly tell the maintainer that the local checkpoint is ready and that `yarn release <version>` is the next external command

6. External actions require explicit maintainer approval immediately before execution

   Never run any of the following until the maintainer explicitly approves that exact action:
   - `yarn release <target-version>`
   - `yarn draft-release v<release-notes-version>`
   - `cargo publish ...`
   - `gh workflow run ...`
   - `gh pr create ...`
   - `gh release ...`
   - `git tag ...`
   - `git push --tag`
   - any other command that mutates GitHub, publishes artifacts, or pushes refs

   When you reach that boundary:
   - show the exact pending command or workflow
   - explain why it is external or irreversible
   - wait for approval
   - if approval is granted, run only the approved action
   - report the outcome before considering any follow-up action

7. Fall back gracefully when automation cannot continue

   If credentials, tools, or permissions are missing:
   - report the blocker
   - explain why automation stopped
   - give the maintainer the concrete shell command needed next

## Guardrails

- Always start with `node scripts/release-preflight.mjs <target-version> --json`
- Keep the maintainer in the Codex console; do not hand the workflow off to a different chat or UI
- Prefer existing repository scripts and workflows over re-implementing release logic in prompt text
- Treat `scripts/release.mjs` and `scripts/draft-release.mjs` as external-action entry points, not inspect-mode helpers
- Do not publish, tag, create PRs, or mutate GitHub resources without explicit approval immediately before the command
- Re-run the helper after local edits so the maintainer gets refreshed readiness, changelog-summary, and handoff commands
- Do not treat the generated release-notes command as the approval boundary for regular or release-candidate releases; the first external boundary is `yarn release <version>` after the local checkpoint commit
- If the helper reports omitted changelog items, surface them before the commit and again before any external action
- Prefer command output over prose checklists when handing work back to Codex or the maintainer
