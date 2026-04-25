## Why

Issue `#2390` points out that `supportClientCodelens` and `compileStatus` are editor-integration flags. They may be known at initialization time, but requiring them to be init-only would unnecessarily narrow client support: some clients only expose this kind of value through workspace configuration and `workspace/didChangeConfiguration`.

Tinymist should therefore treat these flags as configuration-wide rather than init-only session metadata. The current branch's original init-only direction would make Tinymist less portable across editors and would ignore the LSP notification path that some clients rely on to communicate configuration changes.

The actual problem to solve is narrower and more mechanical:

- Tinymist should accept these flags from configuration for clients that only support configuration.
- Tinymist should request and parse the complete set of editor-integration flags during runtime configuration synchronization.
- Tinymist should let runtime configuration notifications update behavior when the client sends a complete effective configuration.
- Tinymist should still allow initialization-time values to seed the starting state before configuration arrives.
- Tinymist should use a project restart boundary when these values change, rather than treating them as ordinary hot-reload fields.
- Shipped editor integrations should return their injected client-capability values when Tinymist asks for them through `workspace/configuration`.

## What Changes

- Keep editor-integration flags such as `supportClientCodelens` and `compileStatus` configuration-wide instead of moving them to an init-only contract.
- Define a clear lifecycle where initialization options provide bootstrap values, while later `workspace/configuration` responses and complete `workspace/didChangeConfiguration` notifications can update the effective configuration.
- Add the missing editor-integration flags to Tinymist's runtime configuration polling list so parse behavior is driven by complete effective values.
- Reload projects when these editor-integration values change, because their consumers should observe a consistent project boundary rather than a partial hot update.
- Update shipped editor integrations so configuration polling returns injected client-capability values such as `supportClientCodelens`.
- Treat the fallback `tinymist.exportPdf` relative-path error mentioned at the end of `#2390` as follow-up work rather than part of this change.

## Capabilities

### New Capabilities

- `configuration-wide-client-options`: Apply editor-integration flags from workspace configuration, honor configuration notifications, use initialization options as bootstrap values, and restart projects when these values change.

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
