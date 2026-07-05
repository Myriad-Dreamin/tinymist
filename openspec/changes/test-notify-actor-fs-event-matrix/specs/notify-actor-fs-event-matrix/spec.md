## ADDED Requirements

### Requirement: Notify actor filesystem event matrix is explicit
Tinymist SHALL define a deterministic notify actor test matrix for low-level watcher inputs and actor messages.

#### Scenario: Matrix covers actor input dimensions
- **WHEN** the notify actor filesystem event matrix is implemented
- **THEN** it MUST account for dependency sync messages, upstream invalidation messages, raw watcher events, and delayed recheck events
- **AND** it MUST account for create, modify, remove, rename-from, rename-to, paired rename, and multi-path watcher event shapes
- **AND** it MUST account for watched dependencies, unwatched files, removed dependencies, and newly watched dependencies
- **AND** it MUST account for non-empty content, unchanged content, empty content, not-found reads, other read errors, and recovery to content

#### Scenario: Matrix maps watcher inputs to user operations
- **WHEN** user-level file operations are represented at the notify actor layer
- **THEN** the matrix MUST include create, edit, remove, rename, atomic save, transient empty write, failed read, dependency removal, dependency re-addition, and upstream invalidation flows
- **AND** each flow MUST identify the raw actor inputs and expected emitted `FilesystemEvent` values

#### Scenario: Platform-specific watcher shapes are accounted for
- **WHEN** different watcher backends can represent the same user operation with different raw event shapes
- **THEN** the matrix MUST either cover each meaningful shape or document why one shape is represented by another deterministic row

### Requirement: Notify actor tests cover deterministic and real watcher inputs
Tinymist SHALL test notify actor behavior with both deterministic injected inputs and ignored-by-default real filesystem watcher inputs.

#### Scenario: Actor reads from injected file state
- **WHEN** a notify actor matrix test needs file content for a watched path
- **THEN** the test MUST supply that content through a deterministic access model or equivalent test seam
- **AND** the test MUST NOT require the file to exist on the host filesystem for core matrix coverage

#### Scenario: Actor receives injected watcher events
- **WHEN** a notify actor matrix test exercises create, modify, remove, or rename behavior
- **THEN** the test MUST inject the corresponding watcher event shape directly or through a fake watcher event source
- **AND** the emitted `FilesystemEvent` values MUST be collected for assertion

#### Scenario: Production watcher wiring remains covered
- **WHEN** deterministic actor tests are added
- **THEN** Tinymist SHOULD add bounded real filesystem watcher tests for the production `watch_deps` wiring
- **AND** those tests SHOULD be ignored by default and run explicitly in CI
- **AND** they SHOULD exercise representative user-level file operations so OS/backend-specific watcher defects can be exposed

### Requirement: Notify actor outputs and watch lifecycle are asserted
Tinymist SHALL assert both emitted filesystem events and watch lifecycle side effects for notify actor event combinations.

#### Scenario: Changed watched file emits update
- **WHEN** a raw watcher event reports a change for a watched dependency and the injected read result differs from the previous content
- **THEN** the actor MUST emit `FilesystemEvent::Update` with the changed path and snapshot
- **AND** the emitted event MUST use `is_sync = false`

#### Scenario: Unchanged content does not emit update
- **WHEN** a raw watcher event reports a change for a watched dependency and the injected read result matches the previous content
- **THEN** the actor MUST NOT emit a filesystem update for that path

#### Scenario: Unwatched path does not emit update
- **WHEN** a raw watcher event reports a change for a path that is not in the watched dependency set
- **THEN** the actor MUST NOT emit a filesystem update for that path

#### Scenario: Dependency sync manages watch set
- **WHEN** the actor receives a dependency sync message with a new dependency set
- **THEN** it MUST add watches for newly depended files
- **AND** it MUST unwatch files that are no longer depended on
- **AND** it MUST emit a sync filesystem update only for changed dependency contents discovered during sync

#### Scenario: Remove or rename-from resets watch state
- **WHEN** a watched path receives a remove-file event or a rename-from event
- **THEN** the actor MUST mark that path as not actively watched so it can be watched again if it remains a dependency
- **AND** it MUST emit the resulting filesystem change when the path content is confirmed as removed or errored

#### Scenario: Transient empty or missing file waits for recheck
- **WHEN** a watched path changes from stable content to empty content, not-found, or another transient read error
- **THEN** the actor MUST defer the emitted filesystem change until the recheck window confirms the unstable state
- **AND** if the path recovers before confirmation, the actor MUST emit the recovered content instead of the transient empty or missing snapshot

#### Scenario: Upstream invalidation emits upstream update
- **WHEN** the actor receives an upstream invalidation message
- **THEN** it MUST refresh watches for the invalidated dependency paths
- **AND** it MUST emit `FilesystemEvent::UpstreamUpdate` carrying the original upstream event and the refreshed changeset
