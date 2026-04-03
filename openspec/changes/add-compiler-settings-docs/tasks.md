## 1. Establish the shared compiler-settings docs structure

- [ ] 1.1 Add a shared compiler-settings documentation page under `docs/tinymist/feature/` and register it in the docs summary.
- [ ] 1.2 Keep the existing friendly VS Code walkthrough text, while adding links from the VS Code, preview, and export docs to the shared compiler-settings guide.
- [ ] 1.3 Add concrete examples that show reproducible font setup with `tinymist.systemFonts = false`, explicit `tinymist.fontPaths`, and supported `tinymist.typstExtraArgs` package/font arguments.

## 2. Generate the embedded font inventory from `typst-assets`

- [ ] 2.1 Add a small Rust tool crate that reads `typst-assets` source code and extracts the embedded font names used by Tinymist docs.
- [ ] 2.2 Wire the generated font inventory into the docs source so the compiler-settings page consumes generated data instead of a hand-maintained list.
- [ ] 2.3 Document the difference between Tinymist's embedded bundle and the official Typst app emoji-font experience, including guidance for adding an emoji font manually.

## 3. Align settings reference text and validate the docs flow

- [ ] 3.1 Update `locales/tinymist-vscode.toml` for `tinymist.systemFonts`, `tinymist.fontPaths`, `tinymist.typstExtraArgs`, and related deprecated preview font settings so the rendered config docs match the shared guide.
- [ ] 3.2 Run the repository's localization and docs checks needed for the touched files and generated outputs.
- [ ] 3.3 Review the rendered docs and generated embedded-font inventory for consistency across frontend, config, preview, and export pages.
