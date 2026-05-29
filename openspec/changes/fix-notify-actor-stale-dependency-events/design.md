## Context

`NotifyActor` maintains a `watched_entries` map for dependency paths. Dependency sync marks entries as seen for the current dependency set, unwatches entries that are no longer seen, and retains old entries for a short lifetime so later re-additions can compare against the previous snapshot.

Real watcher backends do not guarantee that `unwatch` synchronously drains already queued events. On Linux/inotify, for example, a write near an `unwatch` boundary can still be delivered to the actor after `unwatch` returns. That backend behavior is acceptable, but Tinymist should not treat the delivered event as a valid update if the path has already been removed from the current dependency set.

The current actor update path only checks whether a path is present in `watched_entries`. A retained entry for a removed dependency therefore remains able to emit `FilesystemEvent::Update` until it ages out.

## Goals / Non-Goals

**Goals:**
- Prevent queued raw watcher events for removed dependencies from emitting filesystem updates.
- Prevent delayed rechecks for removed dependencies from emitting stale empty, missing, or errored snapshots.
- Preserve retained snapshots for dependency re-addition so sync can still detect content changes that happened while the dependency was inactive.
- Keep remove/rename recovery behavior for paths that remain dependencies.
- Add deterministic coverage for stale queued events and stale pending rechecks.
- Use real filesystem watcher tests only for integration-level confirmation, with explicit actor-ordering barriers where no-update assertions are needed.

**Non-Goals:**
- Requiring `notify` backends to stop all events immediately after `unwatch`.
- Changing how `NotifyActor` debounces transient empty or missing files for active dependencies.
- Removing the retained-entry cache entirely.
- Reworking upstream invalidation semantics beyond stale dependency filtering.

## Decisions

### 1. Filter by current dependency membership, not backend watch state

The actor should distinguish "currently depended on" from "currently watched by the backend". A path can be a current dependency while `watching == false` after a remove or rename-from event, because the actor intentionally allows it to be watched again later. Filtering solely on `watching` would risk dropping valid remove/recovery flows.

The intended filter is current dependency membership. The existing `seen` flag is already updated during dependency sync and used to identify entries that should be unwatched. A removed dependency has `seen == false` while it is retained in the map.

Alternative considered:
- Filter on `watching == true`. Rejected because active dependencies can temporarily have `watching == false` after remove/rename backend behavior.

### 2. Keep retained entries, but make inactive retained entries non-emitting

The actor should continue retaining entries briefly after dependency removal. This lets a future sync re-add the same dependency and compare the new content against the previous snapshot. The fix should only prevent inactive retained entries from emitting events in response to raw watcher input or pending delayed rechecks.

Alternative considered:
- Remove stale entries immediately when dependencies are removed. Rejected because it discards useful previous snapshot state and changes more lifecycle behavior than needed.

### 3. Apply the inactive-entry filter to both raw watcher events and delayed rechecks

Raw watcher events are not the only stale path. A path can enter `EmptyOrRemoval` state, schedule a delayed recheck, then be removed from dependencies before the recheck fires. That delayed recheck must not emit a stale filesystem update after the dependency was removed.

Alternative considered:
- Filter only in raw event handling. Rejected because pending recheck events are self-produced but still represent stale filesystem state for a dependency that is no longer active.

### 4. Keep deterministic tests as the primary semantic oracle

The deterministic actor harness should assert the no-update semantics for removed dependencies because it can control actor state, event ordering, and delayed rechecks exactly. Real filesystem watcher tests can cover the integration path, but any no-update assertion must first establish that the dependency removal sync was processed by the actor.

Alternative considered:
- Assert no event immediately after sending dependency removal and writing the old path in a real watcher test. Rejected because actor message processing and backend event delivery are both asynchronous, making the test assert a race rather than the desired actor state.

## Risks / Trade-offs

- [Filtering uses an overloaded `seen` flag] -> Mitigate by documenting the membership meaning or introducing a clearer field if the implementation becomes ambiguous.
- [Valid active dependency updates could be dropped] -> Mitigate by filtering current dependency membership, not backend watch state, and adding remove/rename recovery tests.
- [Real watcher tests can remain timing-sensitive] -> Mitigate with explicit actor-ordering barriers and keep deterministic tests as the source of semantic truth.
- [Upstream invalidation semantics may interact with membership state] -> Mitigate by reviewing `UpstreamUpdate` paths and adding tests for invalidated paths that are newly added, retained, and removed.

## Migration Plan

1. Add failing deterministic tests for raw events delivered after dependency removal.
2. Add failing deterministic tests for delayed rechecks that fire after dependency removal.
3. Implement a narrow inactive retained-entry filter in `NotifyActor`.
4. Confirm re-addition still emits a sync update when content changed while inactive.
5. Add or adjust a bounded ignored real filesystem watcher test with an actor-ordering barrier.
6. Run focused `tinymist-project` watcher tests with `system,mock` features and the ignored real filesystem watcher tests.

## Open Questions

- Whether to rename `seen` or split it into a clearer current-dependency membership field during implementation.
- Whether upstream invalidation should be treated as a full dependency refresh or as a targeted invalidation over the existing dependency set.
