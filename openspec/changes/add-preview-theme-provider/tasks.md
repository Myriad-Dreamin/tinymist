## 1. Preview Provider Resolution

- [ ] 1.1 Replace the split preview theme settings with `tinymist.preview.provider` in the VS Code configuration schema and extend variable substitution to support `html:` path values.
- [ ] 1.2 Introduce a preview provider resolver that parses `tinymist.preview.provider`, distinguishes `html:` paths from extension ids, and returns HTML, resource roots, and source metadata.
- [ ] 1.3 Enforce trusted-workspace gating, provider compatibility checks, and built-in fallback diagnostics for invalid provider values.

## 2. Preview Integration

- [ ] 2.1 Update preview webview creation and any content-preview sidebar setup to use the resolved provider resource root instead of a hardcoded Tinymist frontend path.
- [ ] 2.2 Invalidate cached preview HTML/source when `tinymist.preview.provider` changes so reopened previews use the latest override.
- [ ] 2.3 Extend the internal preview inspection command to expose resolved provider source metadata for tests.

## 3. Provider Fixture And Tests

- [ ] 3.1 Add a minimal preview theme provider extension under `contrib/` that exports a hello-world HTML theme and declares Tinymist compatibility.
- [ ] 3.2 Update the VS Code test harness to build and load the fixture extension alongside Tinymist during `test:vsc`.
- [ ] 3.3 Add tests under `editors/vscode/src/test` that cover `html:` parsing, extension-id provider selection, and trust/compatibility fallback at the resolver level.
