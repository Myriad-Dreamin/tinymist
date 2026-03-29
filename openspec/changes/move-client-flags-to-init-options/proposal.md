## Why

Issue `#2390` points out that `supportClientCodelens` and `compileStatus` are editor-integration flags. They may be known at initialization time, but requiring them to be init-only would unnecessarily narrow client support: some clients only expose this kind of value through workspace configuration and `workspace/didChangeConfiguration`.

Tinymist should therefore treat these flags as configuration-wide rather than session-scoped. The current proposal's init-only direction would make Tinymist less portable across editors and would ignore the LSP notification path that some clients rely on to communicate configuration changes.

The actual problem to solve is narrower:

- Tinymist should accept these flags from configuration for clients that only support configuration.
- Tinymist should let runtime configuration notifications update behavior when the client sends them.
- Tinymist should still allow initialization-time values to seed the starting state before configuration arrives.

## What Changes

- Keep editor-integration flags such as `supportClientCodelens` and `compileStatus` configuration-wide instead of moving them to an init-only contract.
- Define a clear precedence and lifecycle where initialization options provide bootstrap values, while later `workspace/configuration` responses and `workspace/didChangeConfiguration` notifications can refine the effective configuration.
- Make Tinymist respect runtime notifications for these flags so clients that only support configuration remain fully supported.
- Update shipped editor integrations, fixtures, and docs to describe these flags as configuration-wide values rather than immutable session metadata.
- Treat the fallback `tinymist.exportPdf` relative-path error mentioned at the end of `#2390` as follow-up work rather than part of this change.

## Capabilities

### New Capabilities

- `configuration-wide-client-flags`: Apply editor-integration flags from workspace configuration, honor configuration notifications, and use initialization options only as bootstrap values when present.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist/src/config.rs`
- `crates/tinymist/src/lsp.rs`
- `crates/tinymist/src/lsp/init.rs`
- `crates/tinymist/src/project.rs` and editor-actor wiring that consume session flags
- `editors/vscode/src/extension.shared.ts`
- `editors/vscode/src/lsp.ts`
- Initialization fixtures, configuration-refresh coverage, and LSP smoke tests under `tests/fixtures/` and `tests/e2e/`
- User-facing config docs sourced from `docs/tinymist/config/`
