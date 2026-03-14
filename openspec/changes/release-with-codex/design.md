## Context

Tinymist already has a functioning release pipeline, but the knowledge is spread across maintainer docs, helper scripts, and GitHub workflows:

- `docs/tinymist/release-instruction.typ` describes the human checklist and release sequencing.
- `scripts/release.mjs` creates a release PR, kicks off asset publishing, updates `tinymist-assets`, and pushes a commit.
- `scripts/draft-release.mjs` generates release announcement content and release artifacts metadata.
- `.github/workflows/detect-pr-tag.yml` and the release workflows encode additional expectations around tags, changelog formatting, and publish prerequisites.

Today, a maintainer can ask Codex for help, but the agent has no repo-native release entry point and no explicit safety model. That creates two risks: the agent may miss project-specific checks, or it may overreach into irreversible actions before the maintainer is ready.

This change is cross-cutting because it touches Codex-facing prompts/skills, maintainer documentation, and possibly small helper automation in `scripts/` to make release inspection machine-readable.

## Goals / Non-Goals

**Goals:**
- Provide a single Codex-facing release workflow that starts from repository inspection instead of human memory.
- Encode tinymist's release rules in a form Codex can apply consistently, including version-mode checks, changelog expectations, and release prerequisites.
- Reuse existing release scripts and CI entry points rather than creating a parallel release implementation.
- Introduce an explicit approval boundary before any external or irreversible operation.
- Leave maintainers with a clear summary of what was changed locally, what remains manual, and what command or approval is needed next.

**Non-Goals:**
- Replacing the existing GitHub Actions release pipeline.
- Fully automating publishing, tagging, or release mutation without maintainer confirmation.
- Redefining tinymist version semantics for regular, nightly, or release-candidate builds.
- Solving every nightly-release-specific branch of automation in the first iteration if the shared release-prep path is sufficient.

## Decisions

### 1. Add a repo-local Codex skill/prompt for release preparation

The entry point should live alongside the existing repo-local OpenSpec prompts and skills under `.github/skills/` and `.github/prompts/`. This keeps the workflow versioned with the repository and makes the release procedure discoverable to Codex without relying on external instructions.

Alternative considered:
- Put all release guidance in prose documentation only. Rejected because Codex would still need to reconstruct the workflow from scattered files every time, increasing variance and the chance of missing safety constraints.

### 2. Make release work flow through explicit phases

The workflow should separate release work into three phases:

1. Inspection and planning
2. Local preparation
3. External actions

Inspection gathers facts such as the requested version, inferred release type, file versions, changelog state, dependency pinning, and expected scripts or workflows. Local preparation handles reversible repository edits and generated artifacts. External actions cover anything with side effects outside the current checkout, such as `cargo publish`, `gh pr create`, `gh workflow run`, `gh release`, or `git tag` pushes.

Alternative considered:
- Call `scripts/release.mjs` immediately after the user names a version. Rejected because that script creates GitHub-side effects early, before Codex has reported readiness or obtained a clear go/no-go from the maintainer.

### 3. Reuse existing release automation through thin wrappers or helper commands

Where tinymist already has release logic, the Codex workflow should invoke or prepare those paths instead of reimplementing them in prompt text. If the current scripts are too side-effectful for inspection mode, add small helper commands that expose machine-readable output for:

- release type classification
- version consistency checks
- changelog presence or parseability
- next-step command generation

This keeps one source of truth for release behavior while giving Codex stable inputs.

Alternative considered:
- Teach the prompt to infer everything by reading files ad hoc. Rejected because the logic would be duplicated, fragile, and harder to keep in sync with future release changes.

### 4. Enforce a hard confirmation boundary for external actions

The workflow should never publish crates, create tags, mutate GitHub releases, or start GitHub workflows without an explicit maintainer confirmation immediately before that action. The default output of the workflow should therefore be a release-readiness summary plus the exact next command or action.

Alternative considered:
- End-to-end automation once the initial request is understood. Rejected because release operations are irreversible, depend on credentials and tokens, and often need a final human review of the changelog and target version.

### 5. Keep maintainer docs aligned with the Codex workflow

The workflow should point back to `docs/tinymist/release-instruction.typ` and update that document if new helper commands or steps are introduced. The documentation remains the human-readable source of policy, while the Codex skill becomes the operational path through that policy.

Alternative considered:
- Let the skill diverge from the docs. Rejected because drift between human and agent workflows would create confusion during real releases.

## Risks / Trade-offs

- [Skill drift from scripts or docs] -> Mitigate by reusing existing scripts where possible and updating docs alongside the skill.
- [Over-automation of irreversible steps] -> Mitigate with a mandatory approval boundary and a default inspect-first mode.
- [Release variants add complexity] -> Mitigate by modeling shared checks first and layering nightly or RC-specific behavior on top.
- [Local environment differences block automation] -> Mitigate by reporting blockers clearly and falling back to explicit manual commands.
- [Prompt-only logic becomes brittle] -> Mitigate by adding small machine-readable helper scripts when file scraping proves unstable.

## Migration Plan

1. Add the new release skill/prompt and any required helper script or command.
2. Update maintainer documentation to describe the Codex-assisted path and its approval boundary.
3. Dry-run the workflow against a release-candidate scenario to confirm it can inspect state, propose local edits, and stop before external actions.
4. Adopt the workflow for future releases incrementally; maintainers can continue using the existing manual path if needed during rollout.

## Open Questions

- Should the first version handle both standard releases and nightly releases, or focus on the shared release-preparation path first?
- Should Codex directly edit changelog/version files during preparation, or begin in advisory mode and only edit once the maintainer confirms the computed release plan?
