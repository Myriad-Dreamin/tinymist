## Why

Tinymist's compiler-setting guidance is currently split across the VS Code frontend guide, preview docs, export docs, and settings reference text. That makes it harder for users to understand how fonts, packages, and extra Typst arguments affect the same compiler environment across editing, previewing, and exporting.

The current docs also drift in a few important places: they do not clearly present `tinymist.systemFonts = false` as a reproducibility tool, package-path guidance is thin, and the embedded-font story is hand-discovered instead of sourced from `typst-assets`. A focused docs change now can make these settings easier to trust and easier to maintain.

## What Changes

- Add a shared compiler-settings documentation page in `docs/tinymist/feature/` that explains fonts, packages, and supported `tinymist.typstExtraArgs` examples for the Typst compiler environment used by Tinymist.
- Keep the existing friendly VS Code walkthrough prose in `docs/tinymist/frontend/vscode.typ`, while adding links from that page to the shared compiler-settings guide instead of replacing it with a terse reference.
- Update previewing and exporting docs so they explicitly explain that preview and export use the same compiler font/package environment and point readers at the shared compiler-settings guide.
- Update VS Code settings-reference text for font-related and extra-argument settings so it matches actual behavior, including the reproducible-build guidance that `tinymist.systemFonts = false` helps avoid host-font drift.
- Introduce a small Rust tool crate that reads `typst-assets` source code and extracts the embedded font list for documentation, so the docs do not rely on a hand-maintained inventory.
- Document the embedded-font bundle, the extra emoji-font gap versus the official Typst app experience, and how users can add their own emoji font if they want similar rendering.

## Capabilities

### New Capabilities
- `compiler-settings-docs`: shared user-facing documentation and generated embedded-font inventory for Tinymist compiler settings across frontend, config, preview, and export docs.

### Modified Capabilities

## Impact

- `docs/tinymist/book.typ`
- `docs/tinymist/feature/`
- `docs/tinymist/frontend/vscode.typ`
- `docs/tinymist/feature/preview.typ`
- `docs/tinymist/feature/export.typ`
- `locales/tinymist-vscode.toml`
- A new Rust tool crate in the workspace for extracting embedded font names from `typst-assets` source
- The documentation build/check flow that consumes generated docs inputs
