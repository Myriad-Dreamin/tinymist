## Context

Tinymist already exposes the compiler-setting surface users need, but the explanation is scattered:

- The friendly VS Code walkthrough in `docs/tinymist/frontend/vscode.typ` explains where to click, but not the full compiler-setting model.
- Preview and export docs talk about those features in isolation even though they rely on the same underlying Typst compiler environment.
- The generated config reference is sourced from `locales/tinymist-vscode.toml`, which is the right source of truth for setting descriptions, but it currently does not tell a complete story about reproducibility, packages, or supported extra-argument examples.

There is also no maintained source for the embedded-font list. Tinymist resolves embedded fonts from `typst_assets::fonts()`, but the current docs rely on manual inspection instead of generated data. Because upstream `typst-assets` can change over time, this is a good fit for a small tooling step rather than another hand-maintained list in Typst prose.

## Goals / Non-Goals

**Goals:**
- Add a first-class compiler-settings page that explains how Tinymist configures the Typst compiler for editing, preview, and export.
- Preserve the user-friendly VS Code walkthrough text while linking it to a canonical shared explanation.
- Explain how `tinymist.systemFonts`, `tinymist.fontPaths`, package paths, and supported `tinymist.typstExtraArgs` work together.
- Make reproducible-build guidance explicit, especially that `tinymist.systemFonts = false` helps avoid host-specific font drift.
- Generate the documented embedded-font list from `typst-assets` source through a Rust tool crate instead of maintaining it by hand.

**Non-Goals:**
- Changing Tinymist's compiler behavior or adding new configuration flags.
- Removing the existing friendly VS Code prose in favor of a reference-only page.
- Exhaustively documenting every flag available in external `typst` binaries beyond the subset Tinymist actually parses and supports in its settings.
- Changing the upstream `typst-assets` bundle or the Typst app's font packaging.

## Decisions

### 1. Add a shared `feature/compiler-settings.typ` page as the canonical explanation

The new page should live in `docs/tinymist/feature/` and be registered in the book summary. It becomes the main place to explain compiler inputs such as font paths, system fonts, package paths, package cache paths, certificate paths, and supported `typstExtraArgs` examples.

Alternative considered:
- Keep expanding `frontend/vscode.typ` only. Rejected because compiler settings affect non-VS Code users as well, and preview/export pages also need a shared destination.
- Put everything in the generated config reference. Rejected because generated setting descriptions are useful as reference text, but they are not a friendly narrative page.

### 2. Preserve and enrich the duplicated VS Code walkthrough prose

The current VS Code page should stay explicit and welcoming for users who want step-by-step directions. The change should add links from that prose to the shared compiler-settings page, but should not replace the duplicated instructions with a minimal cross-reference.

Alternative considered:
- Deduplicate the VS Code page aggressively. Rejected because the walkthrough text is intentionally friendly and useful for onboarding.

### 3. Document compiler settings in two tiers: guided settings first, CLI-shaped extras second

The docs should present:

- a straightforward JSON settings example built around `tinymist.systemFonts = false` and `tinymist.fontPaths` for reproducible builds, and
- curated `tinymist.typstExtraArgs` examples for supported arguments such as `--input`, `--root`, `--font-path`, `--ignore-system-fonts`, `--package-path`, `--package-cache-path`, `--creation-timestamp`, and `--cert`.

This keeps the docs aligned with what Tinymist parses today while still teaching users how to map familiar Typst CLI concepts into editor settings.

Alternative considered:
- Describe `typstExtraArgs` as an unrestricted passthrough. Rejected because Tinymist parses its own supported subset and the docs should not over-promise.

### 4. Generate the embedded-font list with a small Rust tool crate that reads `typst-assets` source

The change should introduce a dedicated tool crate whose job is to inspect the `typst-assets` source and extract the font asset names from the embedded font list. That tool should emit a docs-consumable artifact such as Typst or JSON data that the shared compiler-settings page can include.

This approach satisfies two needs at once:

- it keeps the docs synchronized with the actual `typst-assets` source, and
- it avoids inventing a parallel hand-maintained list in the docs tree.

Alternative considered:
- Manually maintain the embedded-font list in Typst docs. Rejected because it will drift.
- Depend on `typst_assets::fonts()` directly at runtime to infer names from bytes. Rejected because the API exposes font data, not a stable font-name inventory, and the requested approach is to read the source code.

### 5. Keep generated config docs sourced from `locales/tinymist-vscode.toml`

The setting descriptions should be updated in the locale source file so that the rendered config reference stays aligned with the shared compiler-settings guide. Generated markdown or package localization bundles should not be edited directly.

Alternative considered:
- Patch generated markdown/config output directly. Rejected because this repo treats generated docs as artifacts.

## Risks / Trade-offs

- [Parsing `typst-assets` source could break when upstream source layout changes] -> Mitigate by keeping the extractor narrow, testing it against the current `typst-assets` layout, and failing loudly when the expected pattern changes.
- [Keeping friendly VS Code prose and a shared page introduces duplication] -> Mitigate by making the shared page the semantic source of truth and using the VS Code page for guided onboarding plus links.
- [Docs may imply support for unsupported extra arguments] -> Mitigate by limiting examples to the argument subset Tinymist parses today and calling that out explicitly.
- [The Typst app emoji-font note can drift over time] -> Mitigate by phrasing it as a documented difference in bundled experience and keeping the wording close to the generated embedded-font inventory rather than a broad promise about all future app packaging.

## Migration Plan

1. Add the shared compiler-settings docs page and register it in the docs summary.
2. Add the Rust tool crate that extracts embedded font names from `typst-assets` source and wire its output into the docs source.
3. Update VS Code, preview, export, and settings-reference text to link to the shared page and adopt the reproducible-build/package guidance.
4. Run the docs and localization checks used by the repository to confirm the generated artifacts stay consistent.
5. Rollback, if needed, is a straightforward revert because the change only affects documentation, supporting tooling, and generated docs inputs.

## Open Questions

- What generated artifact format is most convenient for the docs page to consume: a Typst include file, a JSON blob, or both?
- Where should the tool crate live in the workspace so it fits existing repository conventions for Rust-based docs tooling?
- Whether the docs should name the Typst app's extra emoji font exactly or keep the wording one step more general if upstream packaging changes frequently.
