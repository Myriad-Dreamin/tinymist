## ADDED Requirements

### Requirement: Removed dependencies do not emit stale filesystem updates
Tinymist SHALL prevent retained notify actor entries for removed dependencies from emitting filesystem updates in response to late watcher input or delayed rechecks.

#### Scenario: Queued raw watcher event arrives after dependency removal
- **WHEN** dependency sync removes a path from the current dependency set
- **AND** the actor retains the path entry for cache lifetime purposes
- **AND** a raw watcher event for that path is delivered after the removal
- **THEN** the actor MUST NOT emit `FilesystemEvent::Update` for that removed dependency

#### Scenario: Pending delayed recheck fires after dependency removal
- **WHEN** a path schedules a delayed recheck for transient empty content, missing content, or a read error
- **AND** dependency sync removes that path from the current dependency set before the delayed recheck fires
- **THEN** the delayed recheck MUST NOT emit `FilesystemEvent::Update` for that removed dependency

#### Scenario: Removed dependency can still emit after re-addition
- **WHEN** dependency sync removes a path from the current dependency set
- **AND** the path content changes while it is inactive
- **AND** a later dependency sync re-adds that path
- **THEN** the actor MUST compare the re-added path against the retained previous snapshot
- **AND** the actor MUST emit a sync `FilesystemEvent::Update` when the re-added content differs

#### Scenario: Active remove or rename recovery remains valid
- **WHEN** a current dependency receives a remove-file event or rename-from event
- **THEN** the actor MUST still allow the active dependency to emit the confirmed removal or recovery update according to the existing delayed recheck behavior
- **AND** the stale dependency filter MUST NOT depend solely on backend watch state

#### Scenario: Real watcher late events are tolerated
- **WHEN** a real filesystem watcher backend delivers a queued event after `unwatch` returns
- **THEN** Tinymist MUST treat backend delivery as acceptable
- **AND** the actor MUST enforce removed-dependency no-update semantics from its own dependency state
