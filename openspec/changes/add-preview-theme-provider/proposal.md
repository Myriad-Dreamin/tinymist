## Why

Tinymist currently hardcodes preview HTML to its built-in frontend or the legacy compat bundle, so users cannot point the preview at an alternate frontend shipped by another extension or a local HTML file. We need a safe way to swap preview HTML sources in VS Code so preview theming can evolve independently, while keeping compatibility and workspace trust boundaries explicit without splitting the configuration surface across multiple settings.

## What Changes

- Add a single `tinymist.previewer` setting for VS Code preview theme selection.
- Interpret `tinymist.previewer` values starting with `html:` as a filesystem path to an HTML entry file compatible with `tools/typst-preview-frontend`.
- Interpret any other non-empty `tinymist.previewer` value as an installed extension id that supplies preview HTML.
- Define a preview theme provider contract for VS Code extensions, including compatibility metadata/checking that defaults to exact Tinymist version matching.
- Ignore preview provider overrides in untrusted workspaces and fall back to Tinymist's built-in preview HTML.
- Add a minimal provider test extension under `contrib/` and extend `editors/vscode/src/test` coverage for provider parsing, resolution, and fallback behavior.

## Capabilities

### New Capabilities
- `previewer`: Let the VS Code extension resolve preview HTML from a built-in source, an external provider extension, or a configured local HTML file selected through a single `tinymist.previewer` setting with compatibility and trust checks.

### Modified Capabilities

None.

## Impact

- `editors/vscode/src/features/preview.ts` and `editors/vscode/src/features/preview-compat.ts` will need a shared preview provider resolution path instead of a single hardcoded HTML source.
- `editors/vscode/src/config.ts` and `editors/vscode/package.json` will need the new `tinymist.previewer` setting and provider-string parsing for `html:` paths.
- `editors/vscode/src/test` will need to load an additional extension during e2e runs and assert the resolved preview provider source.
- A new `contrib/` VS Code extension fixture will be added to provide a minimal hello-world preview theme for tests.
