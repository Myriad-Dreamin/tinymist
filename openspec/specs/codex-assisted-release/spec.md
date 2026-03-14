# codex-assisted-release Specification

## Purpose
TBD - created by archiving change release-with-codex. Update Purpose after archive.
## Requirements
### Requirement: Release workflow performs repository preflight
The Codex-assisted release workflow SHALL inspect the repository and summarize release readiness before proposing release edits or external actions.

#### Scenario: Preflight summarizes release state
- **WHEN** a maintainer asks Codex to prepare a tinymist release for a target version
- **THEN** the workflow reports the inferred release type, relevant version-bearing files, changelog status, release entry points, and any unmet prerequisites it can detect from repository state

#### Scenario: Preflight reports blockers before action
- **WHEN** the workflow detects a blocker such as inconsistent versions, missing changelog content, or forbidden release dependency state
- **THEN** it presents the blocker in the readiness summary and does not proceed directly to external release actions

### Requirement: Release workflow prepares reversible repository changes
The Codex-assisted release workflow SHALL support local, reviewable release preparation steps that align with tinymist's existing release conventions.

#### Scenario: Local preparation uses repository conventions
- **WHEN** preflight passes and the maintainer approves local preparation
- **THEN** the workflow updates or drafts local release artifacts such as version-bearing files, changelog entries, release notes, or PR metadata using repository-defined conventions and existing helper automation where available

#### Scenario: Local preparation remains reviewable
- **WHEN** the workflow completes local preparation
- **THEN** it summarizes the files changed, the validations performed, and the remaining release steps before any publish or tag action occurs

### Requirement: External release actions require explicit approval
The Codex-assisted release workflow MUST require an explicit maintainer confirmation immediately before running any action with external side effects.

#### Scenario: No approval means no external side effects
- **WHEN** the next release step would publish artifacts, create or edit GitHub resources, or push a Git tag
- **THEN** the workflow presents the exact pending action and waits for explicit approval instead of executing it automatically

#### Scenario: Approved action is reported clearly
- **WHEN** the maintainer explicitly approves an external release action
- **THEN** the workflow performs only the approved action and reports the outcome or failure details before continuing

### Requirement: Release workflow provides graceful fallback guidance
The Codex-assisted release workflow SHALL provide manual next steps when automation cannot continue safely.

#### Scenario: Missing tooling or credentials block automation
- **WHEN** required tools, authentication, or permissions are unavailable for a release step
- **THEN** the workflow reports the blocker, explains why automation stopped, and gives the maintainer the concrete manual command or checklist item needed to continue

#### Scenario: Existing release automation is preferred
- **WHEN** the workflow needs to generate release metadata or execute release-specific logic already implemented in the repository
- **THEN** it uses the existing script or helper path instead of reimplementing the logic solely in prompt text

