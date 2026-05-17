## Why

Tinymist can receive workspace changes through server-side watchers or through delegated client-side file watching (`tinymist/fs/watch` and `tinymist/fsChange`). The delegated path has distinct ordering, read-error, delete, and sync semantics, and it needs coverage against the same normalized operation model so client-side watch behavior cannot diverge from server-side watcher behavior unnoticed.

## What Changes

- Add a delegated-watch workspace-change matrix for the VS Code/client-side watch ingress and the tinymist server `tinymist/fsChange` request path.
- Cover how client watch requests, read clocks, inserts/removes, read errors, delete events, sync updates, and unwatch behavior normalize into Tinymist filesystem changes.
- Assert that delegated watch inputs produce the same server-visible workspace state and representative LSP response transitions as equivalent normalized filesystem changes.
- Include rows for create, content update, remove, rename-as-delete/create, directory changes, dependency membership changes, read-error recovery, and mixed batches.
- Keep this as transport/ingress coverage. It does not replace notify actor tests or tinymist LSP API class-integration tests.

## Capabilities

### New Capabilities
- `delegated-watch-workspace-change-matrix`: Defines deterministic coverage for client-side watch ingress and `tinymist/fsChange` normalization across modeled workspace changes.

### Modified Capabilities
- None.

## Impact

- Affected areas: `editors/vscode/src/lsp.ts`, `crates/tinymist/src/input.rs`, `crates/tinymist/src/input/watch.rs`, and tinymist test harnesses for custom requests.
- May add TypeScript unit tests for client watch bookkeeping and Rust integration tests for `tinymist/fsChange` request handling.
- Complements `test-notify-actor-fs-event-matrix` and `test-tinymist-lsp-workspace-change-matrix`.
- No production protocol change is intended.
