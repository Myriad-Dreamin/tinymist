## Context

This change is cross-cutting because it spans repository-local Codex assets, Typst developer documentation, and the docs generation workflow. The repository already treats Typst documents under `docs/tinymist` as the source of truth for generated Markdown, and it already uses `scripts/link-docs.mjs` plus `typlite` to derive checked-in Markdown outputs from Typst sources. At the same time, Codex skills require a standalone `SKILL.md` with YAML frontmatter at the very start of the file, which is stricter than the normal generated-doc pattern.

The content itself also needs architectural judgment before implementation. Tinymist exposes multiple layers that can be used to build Rust-based Typst tools, from `CompileOnceArgs` and `WorldProvider` through `WorldComputeGraph`, `Analysis`, and `start_project`. Some of those layers are appropriate for downstream tool authors, while others are internal or explicitly risky as a default recommendation. The skill and its supporting docs should encode those distinctions clearly, using `crates/crityp` and `crates/tinymist-cli` as grounded examples.

## Goals / Non-Goals

**Goals:**
- Make Typst documents under `docs/tinymist/dev` the source of truth for the new skill and its supporting API guidance.
- Generate a single portable `.codex/skills/.../SKILL.md` file from those Typst sources, with valid YAML frontmatter and no required sibling reference files.
- Organize the source material into two authored sections:
  - how to make a tool
  - how to use Tinymist Rust APIs
- Assemble the second section from a dedicated `docs/tinymist/dev/...` directory of smaller Typst documents so the authored source stays modular even though the output is flattened.
- Teach stable, recommended API layers through concrete do and do-not examples for word count, parallel compile, and watch-plus-query scenarios.
- Keep the generated skill aligned with maintained repository practice by drawing examples from `crates/crityp` and `crates/tinymist-cli`.

**Non-Goals:**
- Publishing this material as a user-facing chapter in the main Tinymist docs book.
- Replacing existing crate docs or exhaustively documenting every Tinymist internal API.
- Introducing a new docs generator when the existing Typst-to-Markdown flow can be extended.
- Making the generated skill depend on additional bundled reference files in order to be useful after copy-paste into another repository.

## Decisions

### 1. Keep Typst sources modular, but flatten the generated skill output

The authored source should live under a new `docs/tinymist/dev/...` subtree with one top-level skill entry document and several included subdocuments for the API portion. The generated artifact should still be a single standalone `SKILL.md` under `.codex/skills/`.

This preserves the repository's source-of-truth convention while satisfying the portability requirement for downstream reuse. It also lets the API section grow through focused Typst files without turning the checked-in skill into a hand-edited or fragmented asset.

Alternative considered:
- Author `.codex/skills/.../SKILL.md` directly and only store supplemental API docs in Typst. Rejected because it splits the source of truth and invites drift between the skill body and the richer API guidance.

### 2. Treat the generated skill as a repo-local developer artifact, not a book chapter

The new Typst files should live in `docs/tinymist/dev`, but they do not need to be added to the main docs book summary by default. Their primary purpose is to generate a Codex skill and preserve developer-oriented source material close to the codebase.

This keeps the change scoped to the intended audience and avoids forcing the public documentation navigation to absorb a Codex-specific onboarding document prematurely.

Alternative considered:
- Add the material directly to the public docs book. Rejected because the main consumer is the generated skill, and the resulting prose is likely to be too Codex- and workflow-oriented for general end-user docs.

### 3. Extend the existing Typst-to-Markdown generator instead of creating a separate pipeline

`scripts/link-docs.mjs` should gain a new conversion target for the skill output, reusing the same `typlite`-based conversion path that already generates repository Markdown from Typst sources.

This keeps generation behavior consistent with the rest of the repository and lets validation continue to rely on the existing docs generation entrypoints.

Alternative considered:
- Add a dedicated ad hoc script just for the skill. Rejected because it would duplicate conversion behavior and create another place for Typst-to-Markdown semantics to drift.

### 4. Emit YAML frontmatter and exact markdown through explicit raw output helpers

The skill output must begin with YAML frontmatter at byte zero. The source Typst should therefore use a deliberate raw-output mechanism for frontmatter and any other exact Markdown fragments that cannot tolerate wrapper markup. The existing `typlite` custom verbatim path provides a precedent for that style of output.

This avoids hand-editing the generated skill while preserving strict skill-file requirements. It also gives the source documents a single place to define generation-safe helpers for frontmatter and exact fenced content.

Alternative considered:
- Prepend a generated-file comment before the frontmatter, mirroring other generated docs. Rejected because that would break the skill metadata format.

Alternative considered:
- Post-process the generated Markdown with a custom line-splicing script. Rejected because the Typst source should remain the authoritative representation of the final skill structure.

### 5. Document recommended API layers explicitly and mark unsafe defaults as anti-patterns

The skill should teach Tinymist APIs in layers:
- compile-once setup with `CompileOnceArgs` and `WorldProvider`
- compile and caching with `CompilerWorld` and `WorldComputeGraph`
- semantic queries with `Analysis`
- long-lived project loops with `start_project`
- reusable helpers such as Tinymist's word-count implementation

The guidance should explicitly discourage exposing raw internal file-watcher machinery as the default downstream recipe. The watch-and-query scenario should instead point users toward the higher-level project and analysis flow that Tinymist itself uses.

This keeps the skill actionable for downstream tool authors and encodes the design judgment that exploration surfaced: some internal APIs are real but should not be the first recommendation.

Alternative considered:
- Present all discovered layers neutrally and let the reader infer which ones are stable. Rejected because the goal of the skill is to reduce ambiguity, not mirror the entire internal architecture without curation.

### 6. Use live repository metadata and maintained examples where possible

The Typst source should read repository metadata such as current versions from `Cargo.toml` where helpful for dependency examples, following the same pattern used in existing docs. Example code and anti-examples should be derived from maintained crates such as `crityp` and `tinymist-cli`, but rewritten into concise tutorial-sized snippets that keep the generated skill standalone.

This minimizes version drift in dependency guidance while still keeping the final skill independent of repository-local path lookups after generation.

Alternative considered:
- Hard-code versions and ask maintainers to update the skill manually. Rejected because version-sensitive dependency snippets are exactly the sort of content that generated docs should keep synchronized automatically.

## Risks / Trade-offs

- [The generated skill frontmatter could be malformed by Markdown conversion details] -> Mitigate by using explicit raw-output helpers and validating that the generated file starts with YAML frontmatter exactly.
- [The skill may drift from real Tinymist APIs as examples evolve] -> Mitigate by sourcing examples from maintained crates and using live repository metadata where possible.
- [The docs source tree could become too large for a supposedly portable skill] -> Mitigate by keeping the modularity in Typst source only and flattening the final skill into one file.
- [Downstream readers may overgeneralize internal Tinymist APIs as stable public guidance] -> Mitigate by structuring the API section around recommended layers and explicit do/do-not examples.
- [The new docs generation target could be forgotten during routine updates] -> Mitigate by wiring it into the existing generation script and validating it alongside other generated docs.

## Migration Plan

1. Add the new Typst source files under `docs/tinymist/dev/...`, including the top-level skill entry document and the modular API subdocuments.
2. Extend `scripts/link-docs.mjs` to generate the standalone `.codex/skills/.../SKILL.md` from the new Typst entrypoint.
3. Validate the generated output shape, especially YAML frontmatter, Markdown structure, and the absence of required sibling reference files.
4. Keep the new skill source and generated output under version control so later API or example updates continue to flow through the same Typst-based path.

## Open Questions

- Should the generated skill also emit optional `agents/openai.yaml` metadata now, or should that stay out of scope for the initial change?
- Do we want a dedicated validation command for this generated skill, or is extending the existing docs generation checks sufficient for the first version?
