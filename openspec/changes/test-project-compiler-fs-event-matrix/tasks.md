## 1. Define the project compiler event matrix

- [x] 1.1 Review existing mock VFS/world/project tests and identify reusable fixture patterns.
- [x] 1.2 Define the project compiler filesystem event matrix with dimensions for event variant, sync flag, insert payload, remove payload, path relation, batch shape, and sequence shape.
- [x] 1.3 Map user operations to matrix rows: create, edit, remove, rename, delete-then-recreate, failed-read-then-recovery, transient-empty-write, initial sync, follow-up non-sync update, and unrelated file churn.
- [x] 1.4 Document any unreachable, redundant, or deferred event combinations in the matrix source.

## 2. Build focused project compiler test helpers

- [x] 2.1 Add mock project compiler fixtures that can run full compile when import dependency behavior must be observed.
- [x] 2.2 Add helpers to apply `FilesystemEvent::Update` and `FilesystemEvent::UpstreamUpdate` to `ProjectCompiler` through `Interrupt::Fs`.
- [x] 2.3 Add helpers to drain and inspect `NotifyMessage::SyncDependency` from the mock compiler notify receiver.
- [x] 2.4 Add assertions for compile reasons, VFS freshness, dependency paths, diagnostics or compile output, and harmless VFS decisions.

## 3. Add matrix coverage tests

- [x] 3.1 Test initial sync and follow-up non-sync update behavior with `ignore_first_sync`.
- [x] 3.2 Test create and edit events for entry files, imported dependencies, newly created dependencies, and unrelated files.
- [x] 3.3 Test remove-only, read-error, and empty-content event shapes for depended and unrelated paths.
- [x] 3.4 Test dependency rename flows with import references updated and with old import references left in place.
- [x] 3.5 Test delete-then-recreate and failed-read-then-recovery sequences.
- [x] 3.6 Test multi-file batches, including remove-plus-insert rename-shaped changesets.
- [x] 3.7 Test upstream invalidation events that combine delayed memory changes and filesystem changes.
- [x] 3.8 Test that unrelated filesystem churn can remain harmless without changing dependencies or diagnostics.

## 4. Validate project compiler coverage

- [x] 4.1 Run the focused project compiler tests for `tinymist-project` with the required mock features.
- [x] 4.2 Run focused VFS/world mock tests if new helpers touch `tinymist-vfs` or `tinymist-world` (not needed; helpers stayed test-local in `tinymist-project`).
- [x] 4.3 Run `cargo fmt --check --all`.
- [x] 4.4 Review test output to confirm every matrix row has an explicit expected outcome.
