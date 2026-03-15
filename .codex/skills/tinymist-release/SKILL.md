---
name: tinymist-release
description: Prepare a tinymist release with an inspect-first workflow that reuses the repository's existing release scripts and stops before external side effects until the maintainer explicitly approves them.
license: MIT
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
   - tracked `Cargo.toml` and `package.json` files
   - exact version-bearing lines that match the current release version
   - executable shell commands that apply unified diff patches for those updates
   - executable handoff commands for release notes, release scripts, nightly workflow dispatch, tagging, and pushing

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
   - the tracked manifest scan command
   - the exact patch commands the helper generated
   - the review commands the helper generated
   - which exact external commands or workflows would come later

5. Local preparation is allowed without a separate approval prompt

   Reversible, reviewable checkout-local work is allowed, for example:
   - running the generated patch commands
   - drafting or editing `editors/vscode/CHANGELOG.md`
   - composing the release PR title/body
   - running the generated review commands

   After local preparation:
   - rerun `node scripts/release-preflight.mjs <target-version> --json`
   - summarize which generated commands were used
   - identify the remaining manual or external commands

6. External actions require explicit maintainer approval immediately before execution

   Never run any of the following until the maintainer explicitly approves that exact action:
   - `yarn release <target-version>`
   - `yarn draft-release v<target-version>`
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
- Prefer existing repository scripts and workflows over re-implementing release logic in prompt text
- Treat `scripts/release.mjs` and `scripts/draft-release.mjs` as external-action entry points, not inspect-mode helpers
- Do not publish, tag, create PRs, or mutate GitHub resources without explicit approval immediately before the command
- Re-run the helper after local edits so the maintainer gets refreshed patch and handoff commands
- Prefer command output over prose checklists when handing work back to Codex or the maintainer
