Authored with Codex (GPT-6).

## ADDED Requirements

### Requirement: Successful final caller release reclaims map ownership
Tinymist SHALL remove the canonical map owner after the final external handle is successfully released, including concurrent caller releases.

#### Scenario: Two final callers release concurrently
- **WHEN** the map and two caller handles own one canonical allocation
- **AND** both callers release their handles concurrently
- **THEN** the completed releases MUST leave no map-only allocation retained
- **AND** allocation and destruction counters MUST agree after all handles have been released

#### Scenario: Multiple callers release distinct values
- **WHEN** multiple threads own handles for the same set of distinct canonical values
- **AND** all caller handles are released
- **THEN** each canonical payload MUST be destroyed exactly once
- **AND** the map MUST contain none of those values

### Requirement: Reinterning preserves live canonical identity
Tinymist SHALL preserve canonical pointer identity for equal values while usable caller handles remain, including interning that overlaps cleanup.

#### Scenario: Interning overlaps a candidate final release
- **WHEN** cleanup observes a candidate final caller
- **AND** another caller interns an equal value before removal
- **THEN** cleanup MUST recheck ownership under the removal synchronization protocol
- **AND** it MUST NOT remove the map owner while that new caller still owns the allocation

#### Scenario: Removal encounters colliding values
- **WHEN** distinct canonical allocations share a hash
- **THEN** cleanup MUST remove the intended allocation by pointer identity
- **AND** removal MUST NOT invoke payload equality callbacks under the release lock

### Requirement: Cleanup preserves representation and callback behavior
Tinymist SHALL preserve supported sized/unsized handle and nested-option layouts and keep final payload destruction outside interner locks.

#### Scenario: Recursive payload destruction releases the same shard
- **WHEN** a final payload destructor releases another interned value of the same type and shard
- **THEN** the nested release MUST complete without deadlocking on cleanup's guards

#### Scenario: Shrinking rehash releases a nonfinal handle
- **WHEN** shard shrinking rehashes a key whose hash callback releases a nonfinal handle of the same type
- **THEN** that release MUST complete in the existing shard-only callback environment

#### Scenario: Handle storage is inspected
- **WHEN** the size of a supported sized handle or a `str` handle is compared with its Arc representation
- **THEN** the handle and its optional representation MUST preserve the corresponding Arc and optional-Arc sizes
