# vfs-cache-file-operation-matrix Specification

## Purpose
Define deterministic VFS, world, and compile-cache state coverage for normalized user file-operation classes. The capability keeps path retirement, source freshness, read-error replacement, and compile-cache freshness observable below the LSP layer and above raw watcher translation.

## Requirements
### Requirement: VFS cache matrix tracks modeled file-operation classes
Tinymist SHALL define deterministic VFS/cache test coverage for the normalized file-operation classes in `U_cov`.

#### Scenario: Matrix covers all modeled rows
- **WHEN** the VFS/cache matrix is implemented
- **THEN** it MUST account for rows `O01` through `O20` from `docs/tinymist/dev/notify-rs-file-operation-decomposition.typ`
- **AND** each row MUST be represented by an executable test case or explicitly documented as equivalent, unreachable, platform-specific, or deferred at the VFS/cache boundary

#### Scenario: Matrix preserves row identity
- **WHEN** a test case represents a normalized file-operation row
- **THEN** the test name or table row MUST include the matching `Oxx` identifier
- **AND** failures MUST identify the modeled operation class being checked

### Requirement: VFS state reflects current observable workspace
Tinymist SHALL assert VFS and world state after normalized workspace mutations.

#### Scenario: Insert and content update refresh current source
- **WHEN** a modeled operation creates, updates, atomically replaces, or recreates a relevant path
- **THEN** VFS source lookup for that path MUST observe the current post-operation bytes or read error
- **AND** parsed source state MUST not reuse an older snapshot for changed bytes

#### Scenario: Remove and rename retire stale paths
- **WHEN** a modeled operation removes, renames, moves, or directory-prefix-rewrites a previously relevant path
- **THEN** lookup for the retired path MUST report missing or an explicit read error
- **AND** VFS state MUST NOT continue serving cached source contents for the retired path

#### Scenario: Read-error transitions replace old snapshots
- **WHEN** a modeled operation changes an affected path from readable to unreadable
- **THEN** VFS state MUST store or surface the read error as the current observation
- **AND** the old readable source MUST NOT remain the active source for that path

### Requirement: Compile cache freshness is asserted for affected rows
Tinymist SHALL assert compile-cache behavior for modeled rows that affect entries, dependencies, assets, shadow-open paths, directory prefixes, or mixed batches.

#### Scenario: Affected dependency changes cannot remain silently clean
- **WHEN** a modeled operation changes or retires a path used by the last successful compilation
- **THEN** the compile cache MUST be considered affected or refreshed before later compile results are used
- **AND** a later compile result MUST reflect the current workspace rather than stale cached source

#### Scenario: Updated references follow new paths
- **WHEN** a modeled rename or move row updates imports to point at the new path
- **THEN** the next compile snapshot MUST depend on the new path
- **AND** it MUST NOT retain the old path as an active dependency

#### Scenario: Unrelated churn remains harmless
- **WHEN** a modeled operation affects only unrelated paths
- **THEN** Tinymist MAY treat the change as harmless for compile scheduling
- **AND** the test MUST assert that dependency state, diagnostics, and current compile cache are not corrupted by the skipped change
