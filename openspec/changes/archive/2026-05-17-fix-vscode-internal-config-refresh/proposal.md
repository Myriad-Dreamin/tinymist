## Why

Tinymist's VS Code extension injects several extension-managed configuration fields at startup, including completion trigger flags and internal capability toggles such as `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`. Later workspace configuration refreshes only reflect user-facing VS Code settings, so those internal fields can disappear or reset after a configuration change, matching the bug described in issue `#2378`.

This makes runtime behavior depend on whether the language server has been restarted recently instead of keeping a stable client/server contract for the lifetime of the session.

## What Changes

- Preserve extension-managed internal Tinymist configuration values across runtime configuration refreshes, not only during initial startup.
- Route startup configuration and later configuration synchronization through the same augmentation path so the language server sees a consistent set of internal capability flags.
- Keep existing user-setting processing, including VS Code variable substitution and invalid `tinymist.fontPaths` handling, while adding the missing internal fields to refresh payloads.
- Add VS Code-side regression coverage for configuration refreshes so missing internal fields are caught before release.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `vscode-config-loading`: runtime workspace-configuration refreshes preserve extension-managed internal settings alongside user-configured values.

## Impact

- `editors/vscode/src/config.ts`
- `editors/vscode/src/extension.shared.ts`
- `editors/vscode/src/lsp.ts`
- VS Code extension tests covering configuration loading and refresh behavior
