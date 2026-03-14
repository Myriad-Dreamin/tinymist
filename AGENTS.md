# AGENTS.md

## OpenSpec Workflow

This repository uses OpenSpec for planned work. Prefer the OpenSpec workflow for new features, behavior changes, multi-step fixes, and refactors. Small local edits can still be handled directly when a formal change would add more overhead than value.

## Available Skills

- `openspec-propose`: Create a new OpenSpec change with proposal, design, and tasks. File: `.codex/skills/openspec-propose/SKILL.md`
- `openspec-explore`: Investigate ideas, clarify requirements, and inspect the codebase without implementing. File: `.codex/skills/openspec-explore/SKILL.md`
- `openspec-apply-change`: Implement tasks from an existing OpenSpec change. File: `.codex/skills/openspec-apply-change/SKILL.md`
- `openspec-archive-change`: Archive a completed OpenSpec change after implementation is done. File: `.codex/skills/openspec-archive-change/SKILL.md`

## Repo Layout

- `openspec/config.yaml`: OpenSpec project configuration and artifact rules.
- `openspec/changes/`: Active and archived OpenSpec changes.
- `openspec/specs/`: Accepted specifications.
- `.codex/skills/`: Repo-local Codex skills for the OpenSpec workflow.
