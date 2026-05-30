## 1. Previewer Resolution

- [x] 1.1 Replace the split previewer settings with `tinymist.previewer` in the VS Code configuration schema and extend variable substitution to support `html:` path values.
- [x] 1.2 Introduce a previewer resolver that parses `tinymist.previewer`, distinguishes `html:` paths from extension ids, and returns HTML, resource roots, and source metadata.
- [x] 1.3 Enforce trusted-workspace gating, provider compatibility checks, and built-in fallback diagnostics for invalid provider values.

## 2. Preview Integration

- [x] 2.1 Update preview webview creation and any content-preview sidebar setup to use the resolved provider resource root instead of a hardcoded Tinymist frontend path.
- [x] 2.2 Invalidate cached previewer HTML/source when `tinymist.previewer` changes so reopened previews use the latest override.
- [x] 2.3 Extend the internal preview inspection command to expose resolved provider source metadata for tests.

## 3. Provider Fixture And Tests

- [x] 3.1 Add a minimal previewer provider extension under `contrib/` that exports a hello-world HTML previewer and declares Tinymist compatibility.
- [x] 3.2 Update the VS Code test harness to build and load the fixture extension alongside Tinymist during `test:vsc`.
- [x] 3.3 Add tests under `editors/vscode/src/test` that cover `html:` parsing, extension-id provider selection, and trust/compatibility fallback at the resolver level.

## 4. Previewer Naming Follow-up

- [x] 4.1 Rename PreviewTheme-facing TypeScript APIs and fixtures to Previewer terminology.
- [x] 4.2 Resolve `tinymist.previewer` at runtime and treat `myriad-dreamin.tinymist` as the built-in previewer.
