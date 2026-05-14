## 1. Build a deterministic notify actor harness

- [ ] 1.1 Add a test-only constructor or internal abstraction that lets notify actor tests inject a path access model.
- [ ] 1.2 Add a fake watcher command sink that records watch and unwatch requests without using the host watcher.
- [ ] 1.3 Add a test runner that injects actor messages and raw watcher events and collects emitted `FilesystemEvent` values.
- [ ] 1.4 Add bounded or controlled-time support for delayed recheck tests.

## 2. Define the notify actor event matrix

- [ ] 2.1 Define raw actor input rows for dependency sync, upstream invalidation, raw watcher events, and delayed recheck events.
- [ ] 2.2 Cover create, modify, remove, rename-from, rename-to, paired rename, and multi-path watcher event shapes.
- [ ] 2.3 Cover watched dependencies, unwatched files, removed dependencies, and newly watched dependencies.
- [ ] 2.4 Cover non-empty content, unchanged content, empty content, not-found reads, other read errors, and recovery to content.
- [ ] 2.5 Document backend-specific watcher shapes that are represented by equivalent deterministic rows.

## 3. Add notify actor matrix tests

- [ ] 3.1 Test dependency sync adding watches, removing watches, and emitting sync updates for changed dependency contents.
- [ ] 3.2 Test create and modify events for watched dependencies, including changed and unchanged content.
- [ ] 3.3 Test that raw events for unwatched paths do not emit filesystem updates.
- [ ] 3.4 Test remove-file and rename-from events reset active watch state and emit confirmed file changes.
- [ ] 3.5 Test rename-to, paired rename, and multi-path event shapes.
- [ ] 3.6 Test transient empty content, missing files, read errors, delayed recheck, and recovery before confirmation.
- [ ] 3.7 Test upstream invalidation refreshes watches and emits `FilesystemEvent::UpstreamUpdate` with the original upstream payload.
- [ ] 3.8 Add a bounded production wiring smoke test for `watch_deps` if it can be kept stable.

## 4. Validate notify actor coverage

- [ ] 4.1 Run focused `tinymist-project` tests with the `system` feature enabled.
- [ ] 4.2 Run any focused mock VFS tests if the actor harness reuses or extends `tinymist-vfs::mock`.
- [ ] 4.3 Run `cargo fmt --check --all`.
- [ ] 4.4 Review test output to confirm every notify actor matrix row has an explicit emitted-event and watch-lifecycle expectation.
