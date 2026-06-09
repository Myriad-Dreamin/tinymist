## 1. Reproduce Stale Dependency Events

- [ ] 1.1 Add a deterministic notify actor test where dependency sync removes a path and a later raw watcher event for the retained entry emits no update.
- [ ] 1.2 Add a deterministic notify actor test where a pending delayed recheck fires after dependency removal and emits no update.
- [ ] 1.3 Extend the dependency re-addition test to confirm changed content still emits a sync update after the path becomes active again.

## 2. Implement Actor Filtering

- [ ] 2.1 Review `NotifyActor::update_watches`, `notify_event`, `notify_entry_update`, and `recheck_notify_event` to identify the current dependency membership state used for retained entries.
- [ ] 2.2 Add a narrow inactive retained-entry filter for raw watcher event handling.
- [ ] 2.3 Add the same inactive retained-entry filter for delayed recheck handling.
- [ ] 2.4 Confirm remove-file and rename-from recovery for paths that remain dependencies still emits the existing confirmed updates.

## 3. Real Watcher Coverage

- [ ] 3.1 Add or adjust an ignored real filesystem watcher test that writes a removed dependency only after an actor-ordering barrier confirms the removal sync was processed.
- [ ] 3.2 Keep real watcher assertions focused on actor semantics and avoid requiring `unwatch` to synchronously drain backend event queues.

## 4. Validate

- [ ] 4.1 Run `RUSTFLAGS='-Dwarnings' cargo test -p tinymist-project --features system,mock watch::tests`.
- [ ] 4.2 Run `RUSTFLAGS='-Dwarnings' cargo test -p tinymist-project --features system,mock --locked real_fs_ -- --ignored --nocapture`.
- [ ] 4.3 Run `cargo fmt --check --all`.
- [ ] 4.4 Run `git diff --check`.
