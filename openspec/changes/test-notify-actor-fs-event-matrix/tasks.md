## 1. Build a deterministic notify actor harness

- [x] 1.1 Add a test-only constructor or internal abstraction that lets notify actor tests inject a path access model.
- [x] 1.2 Add a fake watcher command sink that records watch and unwatch requests without using the host watcher.
- [x] 1.3 Add a test runner that injects actor messages and raw watcher events and collects emitted `FilesystemEvent` values.
- [x] 1.4 Add bounded or controlled-time support for delayed recheck tests.

## 2. Define the notify actor event matrix

- [x] 2.1 Define raw actor input rows for dependency sync, upstream invalidation, raw watcher events, and delayed recheck events.
- [x] 2.2 Cover create, modify, remove, rename-from, rename-to, paired rename, and multi-path watcher event shapes.
- [x] 2.3 Cover watched dependencies, unwatched files, removed dependencies, and newly watched dependencies.
- [x] 2.4 Cover non-empty content, unchanged content, empty content, not-found reads, other read errors, and recovery to content.
- [x] 2.5 Document backend-specific watcher shapes that are represented by equivalent deterministic rows.

## 3. Add notify actor matrix tests

- [x] 3.1 Test dependency sync adding watches, removing watches, and emitting sync updates for changed dependency contents.
- [x] 3.2 Test create and modify events for watched dependencies, including changed and unchanged content.
- [x] 3.3 Test that raw events for unwatched paths do not emit filesystem updates.
- [x] 3.4 Test remove-file and rename-from events reset active watch state and emit confirmed file changes.
- [x] 3.5 Test rename-to, paired rename, and multi-path event shapes.
- [x] 3.6 Test transient empty content, missing files, read errors, delayed recheck, and recovery before confirmation.
- [x] 3.7 Test upstream invalidation refreshes watches and emits `FilesystemEvent::UpstreamUpdate` with the original upstream payload.
- [x] 3.8 Add bounded ignored-by-default real filesystem watcher integration tests for `watch_deps` and run them explicitly in CI.

## 4. Validate notify actor coverage

- [x] 4.1 Run focused `tinymist-project` tests with the `system` feature enabled.
- [x] 4.2 Confirm no focused mock VFS tests are needed because the actor harness does not reuse or extend `tinymist-vfs::mock`.
- [x] 4.3 Run `cargo fmt --check --all`.
- [x] 4.4 Review test output to confirm every notify actor matrix row has an explicit emitted-event and watch-lifecycle expectation.
