## ADDED Requirements

### Requirement: Project compiler filesystem event matrix is explicit
Tinymist SHALL define a deterministic project compiler test matrix for runtime-facing filesystem events produced by user file operations.

#### Scenario: Matrix covers event dimensions
- **WHEN** the project compiler filesystem event matrix is implemented
- **THEN** it MUST account for `FilesystemEvent::Update` and `FilesystemEvent::UpstreamUpdate`
- **AND** it MUST account for sync and non-sync update events
- **AND** it MUST account for insert-only, remove-only, insert-plus-remove, multi-file, and empty changesets
- **AND** it MUST account for successful content, empty content, and read-error file snapshots
- **AND** it MUST account for entry files, imported dependencies, previously depended paths, newly created dependencies, and unrelated files

#### Scenario: Matrix maps user operations to event shapes
- **WHEN** user-level file operations are represented in project compiler tests
- **THEN** the matrix MUST include create, edit, remove, rename, delete-then-recreate, failed-read-then-recovery, transient-empty-write, initial sync, follow-up non-sync update, and unrelated file churn flows
- **AND** each flow MUST identify the `FileChangeSet` and `FilesystemEvent` shape delivered to `ProjectCompiler`

#### Scenario: Matrix documents omitted combinations
- **WHEN** a possible event combination is not covered by an executable test
- **THEN** the implementation MUST document whether the combination is unreachable, redundant with another matrix row, or intentionally deferred

### Requirement: Project compiler tests drive filesystem events through runtime entry points
Tinymist SHALL test project compiler behavior by delivering matrix events through the same filesystem interrupt path used by the runtime.

#### Scenario: Mock filesystem event reaches ProjectCompiler
- **WHEN** a mock workspace mutation produces a `FileChangeSet`
- **THEN** the test MUST deliver it to `ProjectCompiler` as `Interrupt::Fs(FilesystemEvent::Update(...))`
- **AND** the test MUST avoid directly mutating compiler internals to simulate the resulting filesystem state

#### Scenario: Upstream update reaches ProjectCompiler
- **WHEN** a matrix row models delayed filesystem invalidation around an upstream memory event
- **THEN** the test MUST deliver it as `FilesystemEvent::UpstreamUpdate`
- **AND** the test MUST assert that delayed memory changes and filesystem changes are applied in the expected order

#### Scenario: Dependency sync is observable
- **WHEN** a project compilation completes after a matrix event
- **THEN** the test MUST observe the emitted `NotifyMessage::SyncDependency`
- **AND** the observed dependencies MUST match the current project dependency paths after that compile

### Requirement: Project compiler outcomes are asserted for filesystem event combinations
Tinymist SHALL assert compiler-visible outcomes for each covered filesystem event combination.

#### Scenario: Depended file edit refreshes compiler state
- **WHEN** a filesystem event updates a file used by the last successful compilation
- **THEN** the project compiler MUST mark the project as affected by filesystem events
- **AND** the next compiler result MUST reflect the updated content

#### Scenario: Removed dependency does not remain silently clean
- **WHEN** a filesystem event removes or retires a path used by the last successful compilation
- **THEN** the project compiler test MUST prove the change is not treated as a harmless VFS-only change
- **AND** a later compiler result MUST reflect the current workspace state rather than stale contents for the retired path

#### Scenario: Renamed dependency follows current references or reports missing old references
- **WHEN** a filesystem event renames a depended path and a follow-up edit either updates or does not update the import reference
- **THEN** the compiler result MUST either follow the new path when references are updated or report the old path as unavailable when references are not updated
- **AND** it MUST NOT continue using cached source contents from the retired path

#### Scenario: Unrelated filesystem churn can remain harmless
- **WHEN** a filesystem event changes only files unrelated to the last successful compilation
- **THEN** the project compiler MAY skip recompilation as a harmless VFS change
- **AND** the test MUST assert that this skip does not alter current dependency state or diagnostics

#### Scenario: Initial sync obeys ignore-first-sync semantics
- **WHEN** the project compiler receives an initial sync filesystem event while configured to ignore the first sync
- **THEN** the test MUST assert that the sync event does not create an unintended compile reason
- **AND** a later non-sync event for an affected path MUST still create the expected filesystem compile reason
