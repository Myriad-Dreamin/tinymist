## Why

Issue `#2390` points out that `supportClientCodelens` and `compileStatus` describe the LSP client session, not ordinary workspace settings. Tinymist already accepts both through `initializationOptions`, but it still mixes them into the regular `workspace/configuration` / `workspace/didChangeConfiguration` flow.

That mixing has two bad consequences:

- A later config refresh can overwrite initialize-time client metadata with `null` or default values from workspace settings, even when the client never meant those values to be user-editable runtime config.
- The same fragile path already covers other injected client-only flags such as `triggerSuggest`, `triggerParameterHints`, `triggerSuggestAndParameterHints`, `supportHtmlInMarkdown`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`, so `#2390` is part of a broader class of session-config bugs.

## What Changes

- Introduce an explicit session-scoped initialize-time option surface for client capability flags and compile-status opt-in.
- Restrict runtime `workspace/configuration` polling and `workspace/didChangeConfiguration` updates to true workspace settings, excluding session-scoped options.
- Keep compile-status emission, code-lens behavior, and other client-integrated server behavior stable for the lifetime of an LSP session once initialization completes.
- Update shipped editor integrations, fixtures, and docs so they pass and describe these values as initialize-time session options rather than hot-reloadable workspace config.
- Treat the fallback `tinymist.exportPdf` relative-path error mentioned at the end of `#2390` as follow-up work rather than part of this change.

## Capabilities

### New Capabilities

- `session-scoped-client-options`: Apply client capability metadata and compile-status opt-in only during LSP initialization, not through later workspace configuration refreshes.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist/src/config.rs`
- `crates/tinymist/src/lsp.rs`
- `crates/tinymist/src/lsp/init.rs`
- `crates/tinymist/src/project.rs` and editor-actor wiring that consume session flags
- `editors/vscode/src/extension.shared.ts`
- `editors/vscode/src/lsp.ts`
- Initialization fixtures and LSP smoke coverage under `tests/fixtures/` and `tests/e2e/`
- User-facing config docs sourced from `docs/tinymist/config/`
