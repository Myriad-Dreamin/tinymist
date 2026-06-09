## Why

`NotifyActor` can emit stale filesystem updates for dependencies that were just removed from the current dependency set. Real watcher backends such as inotify may still deliver queued events after `unwatch` returns, and the actor currently keeps removed entries in `watched_entries` long enough for those late events to be interpreted as valid dependency changes.

This should be fixed in the actor state machine rather than by relying on backend-specific `unwatch` timing. The real watcher tests should expose this class of backend behavior, while deterministic actor tests should define the intended Tinymist semantics.

## What Changes

- Treat retained entries for removed dependencies as inactive for raw watcher events and delayed rechecks.
- Preserve the retained snapshot cache so re-adding the same dependency can still compare against the previous snapshot and emit a sync update if content changed while removed.
- Add deterministic actor tests for late raw watcher events after dependency removal.
- Add deterministic actor tests for pending delayed rechecks after dependency removal.
- Optionally re-enable a bounded real filesystem assertion that writes a removed dependency after an actor ordering barrier and verifies no update is emitted for that path.
- Keep backend-specific queued event delivery visible in real watcher integration tests; do not require `unwatch` to synchronously drain the backend queue.

## Capabilities

### New Capabilities

- `notify-actor-stale-dependency-events`: Defines how `NotifyActor` handles queued watcher events and delayed rechecks for dependencies that have been removed from the current dependency set.

### Modified Capabilities

- None.

## Impact

- Affected code: `crates/tinymist-project/src/watch.rs`.
- Affected tests: notify actor deterministic tests and ignored real filesystem watcher tests in `tinymist-project`.
- No public API change is expected.
- No dependency changes are expected.
