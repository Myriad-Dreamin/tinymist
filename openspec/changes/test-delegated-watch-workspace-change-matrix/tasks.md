## 1. Define delegated watch matrix rows

- [ ] 1.1 Map `O01..O20` to delegated client watch ingress support, including unsupported or represented-by rows.
- [ ] 1.2 Define expected `tinymist/fsChange` payloads for create, change, delete, rename-as-delete/create, read-error recovery, dependency membership changes, directory changes, and mixed batches.
- [ ] 1.3 Document how sync and non-sync delegated events differ from server-side watcher events.

## 2. Add client-side watch tests

- [ ] 2.1 Extract or expose testable delegated watch bookkeeping helpers if needed.
- [ ] 2.2 Test watch insertion, initial sync reads, successful file reads, and read-error inserts.
- [ ] 2.3 Test unwatch, delete handling, and stale-read suppression.
- [ ] 2.4 Test watch-clock ordering for racing reads of the same URI.

## 3. Add server fsChange normalization tests

- [ ] 3.1 Test `tinymist/fsChange` insert-only, remove-only, insert-plus-remove, read-error, and mixed payload handling.
- [ ] 3.2 Test sync and non-sync request application to project/server state.
- [ ] 3.3 Test delegated rename-as-delete/create retires old depended paths and exposes new referenced paths.
- [ ] 3.4 Test delegated read-error recovery replaces stale content and later recovers.

## 4. Add representative LSP smoke checks

- [ ] 4.1 Add a representative definition or hover check after delegated dependency create/update/rename.
- [ ] 4.2 Add a representative diagnostics check after delegated remove/read-error/recovery rows.
- [ ] 4.3 Add a representative semantic token or document-symbol check after delegated current-file update where applicable.

## 5. Validate

- [ ] 5.1 Run focused VS Code TypeScript tests for delegated watch bookkeeping.
- [ ] 5.2 Run focused `cargo test -p tinymist` tests for server `tinymist/fsChange` handling.
- [ ] 5.3 Run `yarn lint` or the smallest relevant editor validation command if TypeScript code is touched.
- [ ] 5.4 Run `cargo fmt --check --all`.
