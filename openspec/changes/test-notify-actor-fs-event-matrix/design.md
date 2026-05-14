## Context

`NotifyActor` is responsible for watching dependency paths, reading file contents through an access model, and emitting Tinymist `FilesystemEvent` values. It currently owns state for watched entries, transient empty or missing files, delayed rechecks, dependency sync, and upstream invalidation.

The actor is hard to test because it is coupled to `RecommendedWatcher` and `SystemAccessModel`. That makes deterministic tests for rename, remove, empty-write, and failed-read combinations difficult, and it pushes too much responsibility onto manual testing or higher-level project compiler tests.

This proposal isolates notify actor behavior. It complements, but does not replace, the project compiler filesystem event matrix.

## Goals / Non-Goals

**Goals:**
- Define a deterministic matrix for low-level watcher events and actor messages.
- Test that `NotifyActor` emits the correct `FilesystemEvent` values for each matrix row.
- Test watch lifecycle behavior: add watch, unwatch, dependency removal, rename/remove recovery, and stale entry cleanup.
- Test unstable file handling: transient empty content, missing files, read errors, delayed recheck, and recovery.

**Non-Goals:**
- Testing project compiler results after emitted events. That belongs to `test-project-compiler-fs-event-matrix`.
- Depending on real host filesystem timing or backend-specific watcher behavior for core coverage.
- Changing production watcher policy or fixing rename/cache bugs in this proposal.
- Testing VS Code client-side watch request handling.

## Decisions

### 1. Introduce a deterministic actor test harness

Tests should be able to construct the actor with:

- a mock or injected path access model
- a fake watcher command sink for watch and unwatch assertions
- an injected stream of low-level watcher events and actor messages
- a collected list of emitted `FilesystemEvent` values

This can be done with test-only constructors or a small internal abstraction. The production `watch_deps` entry point should continue using `RecommendedWatcher` and `SystemAccessModel`.

Alternative considered:
- Use temp directories and the real `notify` backend for all actor tests. Rejected because backend timing and event coalescing would make the matrix flaky and platform-sensitive.

### 2. Define the notify actor matrix at the raw watcher boundary

The actor matrix should cover the inputs the actor actually consumes:

| Dimension | Values to cover |
| --- | --- |
| Actor input | `SyncDependency`, `UpstreamUpdate`, raw notify event, delayed recheck |
| Notify kind | create, modify data, remove file, rename-from, rename-to, rename-both or paired rename |
| Path relation | watched dependency, unwatched file, removed dependency, newly watched dependency |
| Read result | non-empty content, unchanged content, empty content, `NotFound`, other read error, recovery to content |
| Entry state | stable, empty-or-removal pending recheck, no previous content, previously errored |
| Batch shape | one path, two rename paths, multiple changed paths |

The matrix should document platform-specific raw notify shapes that are represented by equivalent rows.

Alternative considered:
- Reuse only the user-operation matrix from project compiler tests. Rejected because actor correctness depends on lower-level details such as `RenameMode::From`, missing-file debounce, and watch/unwatch state.

### 3. Assert emitted runtime events and internal watch state

The tests should assert both external and internal actor behavior:

- emitted `FilesystemEvent::Update` changesets
- emitted `FilesystemEvent::UpstreamUpdate` changesets and carried upstream event
- no event emitted for unchanged content or unwatched paths
- delayed event emitted only after an unstable empty or missing file remains unstable
- watch command issued when a dependency becomes watched
- unwatch command issued when a dependency is removed or a rename/remove event invalidates the watch
- rewatch allowed when a still-depended path recovers after rename/remove

Alternative considered:
- Only assert emitted changesets. Rejected because several known watcher problems involve incorrect watch state even when one emitted changeset looks correct.

### 4. Keep real watcher coverage as a smoke test

The matrix should be deterministic and fake-driven. A small tempdir smoke test can remain useful to confirm the production constructor is wired, but it should not be the primary correctness signal.

Alternative considered:
- No real watcher test at all. Rejected because production wiring can still regress even when the pure actor harness passes.

## Risks / Trade-offs

- [Test seam could leak into production API] -> Mitigate by keeping constructors and fake watcher types crate-private or `#[cfg(test)]`.
- [Actor refactor could accidentally change behavior] -> Mitigate by landing harness changes with behavior-preserving tests first.
- [Debounce tests can be slow or flaky] -> Mitigate by using controlled time or bounded async waiting, and isolate real sleeps to the smallest number of tests.
- [Notify backends emit different rename shapes] -> Mitigate by representing backend variants as equivalent matrix rows and keeping the real backend check as smoke coverage only.

## Migration Plan

1. Add a deterministic actor harness that can inject access results, watcher events, and actor messages.
2. Encode the notify actor filesystem event matrix as table-driven tests.
3. Assert emitted `FilesystemEvent` values and watch lifecycle side effects for each row.
4. Add one optional tempdir smoke test for the production `watch_deps` path if it can be kept stable.
5. Run focused `tinymist-project` tests with the `system` feature.
6. Rollback is a straightforward revert because this proposal only adds tests and test-only seams.

## Open Questions

- Whether to expose a generic `NotifyActor` over `PathAccessModel` in production code or keep generic construction behind `#[cfg(test)]`.
- Whether the delayed recheck should use injectable time in this proposal or remain tested with bounded real time initially.
