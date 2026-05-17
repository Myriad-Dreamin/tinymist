## ADDED Requirements

### Requirement: Mock workspace tests can create deterministic compiler runtimes
Tinymist SHALL provide shared Rust test support that lets tests create VFS- and world-backed compiler runtimes from an in-memory workspace without depending on the host filesystem, package registry, or system-font discovery.

#### Scenario: Build a world from in-memory files
- **WHEN** a Rust test defines a workspace root, an entry file, and one or more Typst source files entirely in memory
- **THEN** the shared test support can construct a Tinymist runtime that reads those files through the normal VFS/world path
- **AND** the test does not need to write the workspace to disk or contact external package services to exercise that runtime

### Requirement: Mock workspace tests can model file manipulation flows
Tinymist SHALL provide shared Rust test support that lets tests apply file-manipulation operations to the in-memory workspace and drive the corresponding runtime-facing change flow for create, update, rename, and remove sequences.

#### Scenario: Rename a file inside the mock workspace
- **WHEN** a Rust test renames an in-memory workspace file such as `content.typ` to `new_name.typ`
- **THEN** the shared test support can deliver the resulting path change to the runtime in the same change shape that Tinymist uses for filesystem-driven invalidation
- **AND** the test can observe the runtime state after the rename without recreating the entire workspace by hand

#### Scenario: Follow-up mutation after file removal or rename
- **WHEN** a Rust test removes or renames a file and then applies a later update to a remaining file
- **THEN** the shared test support can drive that sequence through the runtime without falling back to ad hoc per-test mocking
- **AND** the test can assert compilation, dependency, or cache-related outcomes from the full mutation sequence
