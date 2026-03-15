## MODIFIED Requirements

### Requirement: Release workflow performs repository preflight
The Codex-assisted release workflow SHALL inspect the repository and summarize release readiness before proposing release edits or external actions. The preflight summary SHALL report the inferred release type, expected branch name, current branch status, relevant version-bearing files, release-sensitive non-manifest files, changelog status, generated-document follow-up commands, release entry points, and any unmet prerequisites it can detect from repository state.

#### Scenario: Preflight summarizes release state
- **WHEN** a maintainer asks Codex to prepare a tinymist release for a target version
- **THEN** the workflow reports the inferred release type, expected `bump-version-<version>` branch name, current branch status, relevant version-bearing files, release-sensitive non-manifest files, changelog status, generated-document follow-up commands, release entry points, and any unmet prerequisites it can detect from repository state

#### Scenario: Preflight reports changelog coverage
- **WHEN** the workflow can compare candidate release-note items with the current changelog entry for the target version
- **THEN** it reports which candidate items are represented in the changelog and which candidate items are omitted for maintainer review

#### Scenario: Preflight reports blockers before action
- **WHEN** the workflow detects a blocker such as branch mismatch, inconsistent versions, missing changelog content, or stale required generated release documents
- **THEN** it presents the blocker in the readiness summary and does not proceed directly to local commit or external release actions

### Requirement: Release workflow prepares reversible repository changes
The Codex-assisted release workflow SHALL support local, reviewable release preparation steps that align with tinymist's existing release conventions. Local preparation SHALL cover release artifacts such as version-bearing files, changelog entries, generated-document outputs, review commands, and the expected local release commit.

#### Scenario: Local preparation uses repository conventions
- **WHEN** preflight passes and the maintainer approves local preparation
- **THEN** the workflow updates or drafts local release artifacts such as version-bearing files, changelog entries, generated-document outputs, release notes, or PR metadata using repository-defined conventions and existing helper automation where available

#### Scenario: Local preparation creates the release checkpoint commit
- **WHEN** the prepared local release changes have been reviewed and are ready for a checkpoint
- **THEN** the workflow stages the prepared local changes and creates a local commit named `build: bump version to <version>` before presenting `yarn release <version>` as the next external action

#### Scenario: Local preparation remains reviewable
- **WHEN** the workflow completes local preparation
- **THEN** it summarizes the files changed, the validations performed, whether generated-document follow-up commands such as `node scripts/link-docs.mjs` were run, and the remaining release steps before any publish or tag action occurs

### Requirement: External release actions require explicit approval
The Codex-assisted release workflow MUST require an explicit maintainer confirmation immediately before running any action with external side effects. For regular and release-candidate releases, the workflow MUST present `yarn release <version>` as an explicit approval boundary after the local preparation checkpoint and before any GitHub-side mutation, publishing action, or remote push triggered by that command.

#### Scenario: No approval means no external side effects
- **WHEN** the next release step would run `yarn release <version>`, publish artifacts, create or edit GitHub resources, or push a Git tag
- **THEN** the workflow presents the exact pending action and waits for explicit approval instead of executing it automatically

#### Scenario: Release command is handed off explicitly
- **WHEN** the workflow completes local preparation for a regular or release-candidate release
- **THEN** it presents `yarn release <version>` as the next pending external action and does not run that command until the maintainer explicitly approves it

#### Scenario: Approved action is reported clearly
- **WHEN** the maintainer explicitly approves an external release action
- **THEN** the workflow performs only the approved action and reports the outcome or failure details before continuing
