## Why

`NotifyActor` is the layer that turns low-level watcher notifications into Tinymist filesystem events, but its behavior is currently difficult to test because it is tied to the system watcher and real filesystem reads. We need direct coverage for watcher event combinations before depending on it for broader rename and cache-invalidation fixes.

## What Changes

- Define notify-actor tests for the filesystem event combinations that correspond to common user operations: create, modify, delete, rename-from, rename-to, rename pairs, atomic save sequences, transient empty writes, failed reads, dependency sync, dependency removal, and upstream invalidation.
- Add a deterministic actor test harness that can inject file contents and watcher events without depending on the host filesystem or `notify-rs` timing.
- Assert emitted `FilesystemEvent` values, watch/unwatch state transitions, debounce/recheck behavior for unstable empty or missing files, and recovery after rename/remove events.
- Keep this as test coverage and harness work only; it does not change user-facing watcher behavior by itself.

## Capabilities

### New Capabilities
- `notify-actor-fs-event-matrix`: Defines deterministic notify actor coverage for low-level watcher event combinations and their emitted Tinymist filesystem events.

### Modified Capabilities
- None.

## Impact

- Affected Rust areas: primarily `crates/tinymist-project/src/watch.rs`, with possible test-only reuse of `tinymist-vfs::mock`.
- May introduce test-only constructors or internal abstractions so `NotifyActor` can run against a mock access model and injected watcher events.
- Tests should avoid real sleeps where possible; if the existing recheck delay must be exercised, it should use bounded async time control.
- No production watcher policy, editor integration, public runtime API, dependency pin, or generated documentation change is intended.
