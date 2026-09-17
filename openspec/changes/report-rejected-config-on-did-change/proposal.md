## Why

A setting rejected during `workspace/didChangeConfiguration` handling is never reported to the
user (issue `#2715`). `Config::update_by_map` records deserialization failures in
`Config::warnings`, but only the initialization path and the `workspace/configuration` pull
callback display them. The push path (`on_changed_configuration`) never does, and the next
`update_by_map` clears the warnings before anyone sees them. When the whole update is rejected
(e.g. a non-absolute `rootPath`), the rollback also drops the collected warnings, and the
returned error never reaches the client because the notification protocol discards it, so the
user's configuration silently falls back to defaults.

## What Changes

- Show the collected configuration warnings to the client from `on_changed_configuration`, the
  single point that every runtime configuration synchronization path flows through.
- Collapse the duplicate display in the `workspace/configuration` pull callback into that single
  site, so each rejected key is still reported exactly once.
- On the rollback path, report the rejection reason together with the warnings collected by the
  refused update before restoring the old configuration, so a fully rejected payload also
  surfaces a user-facing message.
- Add LSP-level regression tests asserting that push and pull paths each emit exactly one
  `window/showMessageRequest` per rejected key, that valid values emit none, and that rollback
  does not replay old warnings.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `configuration-wide-client-options`: runtime configuration synchronization reports rejected
  settings to the client on every delivery path.

## Impact

- `crates/tinymist/src/lsp.rs`
- `crates/tinymist/src/server.rs`
- `crates/tinymist/src/lsp/workspace_change_tests.rs`
