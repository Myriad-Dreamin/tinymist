
## 1. Add the Codex release workflow

- [x] 1.1 Create a repo-local Codex skill (`/tinymist-dev:release v0.14.x`) and matching prompt entry for release preparation under `.codex/skills/` and `.codex/prompts/`.
- [x] 1.2 Encode the inspect -> local prep -> external action phases, including the mandatory approval boundary before publishing, tagging, or GitHub-side mutations.
- [x] 1.3 Integrate the workflow with existing release scripts or helper commands so local preparation and fallback guidance stay aligned with tinymist's current release path.

## 2. Document and validate the maintainer experience

- [x] 2.1 Update `docs/tinymist/release-instruction.typ` with the Codex-assisted release path, including what Codex can do automatically and what still requires maintainer approval.
- [x] 2.2 Dry-run the workflow for a representative release candidate or nightly scenario and fix any gaps in readiness reporting, local edits, or manual handoff guidance.
