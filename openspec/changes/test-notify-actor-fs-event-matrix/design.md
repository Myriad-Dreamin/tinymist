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
- Add ignored-by-default real filesystem watcher coverage that runs in CI and exercises representative user flows through `RecommendedWatcher`, so OS/backend-specific watcher defects can be surfaced.

**Non-Goals:**
- Testing project compiler results after emitted events. That belongs to `test-project-compiler-fs-event-matrix`.
- Making ordinary local test runs depend on host filesystem watcher timing.
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
- Use only temp directories and the real `notify` backend for all actor tests. Rejected as the sole mechanism because raw backend timing and event coalescing can obscure actor-level regressions, but accepted as a CI-run integration layer for exposing platform-specific watcher defects.

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

### 4. Run real watcher coverage as an ignored CI integration matrix

The actor-boundary matrix should remain deterministic and fake-driven for raw event shapes, delayed recheck timing, and internal watch lifecycle assertions. In addition, Tinymist should run ignored-by-default real filesystem watcher tests through `watch_deps`, temp directories, and actual file operations. These tests should be executed explicitly in CI so Linux, macOS, and Windows notify backend behavior can expose real integration defects.

The real watcher rows should focus on user-level operations rather than exact raw notify shapes:

- dependency sync and newly added dependencies
- edits to watched files and ignored unwatched files
- remove, rename-away, and re-addition flows
- atomic replacement, transient empty writes, missing files, and recovery
- upstream invalidation refresh

Alternative considered:
- Keep only a single real watcher smoke test. Rejected because production wiring can pass while backend-specific operation flows still regress.

## Risks / Trade-offs

- [Test seam could leak into production API] -> Mitigate by keeping constructors and fake watcher types crate-private or `#[cfg(test)]`.
- [Actor refactor could accidentally change behavior] -> Mitigate by landing harness changes with behavior-preserving tests first.
- [Debounce tests can be slow or flaky] -> Mitigate by using controlled time for deterministic actor tests and bounded async waiting for ignored real watcher tests.
- [Notify backends emit different rename shapes] -> Mitigate by representing backend variants as equivalent deterministic rows and by running real watcher integration rows in CI to expose platform-specific behavior differences instead of hiding them.
- [Real watcher tests can be resource-intensive locally] -> Mitigate by marking them `#[ignore]` and running them explicitly in CI.

## Migration Plan

1. Add a deterministic actor harness that can inject access results, watcher events, and actor messages.
2. Encode the notify actor filesystem event matrix as table-driven tests.
3. Assert emitted `FilesystemEvent` values and watch lifecycle side effects for each row.
4. Add ignored-by-default tempdir tests that exercise representative filesystem operations through the production `watch_deps` path and real watcher backend.
5. Run focused `tinymist-project` tests with the `system` feature, and run the ignored real filesystem watcher tests explicitly in CI.
6. Rollback is a straightforward revert because this proposal only adds tests and test-only seams.

## Open Questions

- Whether to expose a generic `NotifyActor` over `PathAccessModel` in production code or keep generic construction behind `#[cfg(test)]`.
- Whether the delayed recheck should use injectable time in this proposal or remain tested with bounded real time initially.
