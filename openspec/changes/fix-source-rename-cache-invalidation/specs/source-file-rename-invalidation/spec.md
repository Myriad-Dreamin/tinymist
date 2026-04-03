## ADDED Requirements

### Requirement: Retired dependency paths are not reused from cache
Tinymist SHALL stop using cached source bytes and parsed source state for a Typst source path after that path has been renamed or removed from the workspace and was part of the last successful compilation's dependency set.

#### Scenario: Renamed dependency is not served from stale cache
- **WHEN** the last successful compilation of `base.typ` depended on `content.typ` and the workspace renames `content.typ` to `new_name.typ`
- **THEN** a later compilation of `base.typ` MUST NOT read the old cached contents of `content.typ`
- **AND** the compile result reflects the current workspace state, either by following updated references to `new_name.typ` or by reporting that `content.typ` is no longer available

#### Scenario: Follow-up updates after rename stay fresh
- **WHEN** a depended source path has been renamed or removed and a user then edits a remaining document such as `base.typ` or the renamed file
- **THEN** Tinymist recompiles using the current filesystem and in-memory contents
- **AND** preview and diagnostics MUST NOT continue showing results derived from the retired path's cached contents

### Requirement: Retired dependency paths force recompilation
Tinymist SHALL treat a rename or removal of a path used by the last successful compilation as a dependency-affecting VFS change rather than a harmless VFS change.

#### Scenario: Removed dependency path forces recompilation
- **WHEN** filesystem updates report that a path used by the last successful compilation has been removed or renamed away
- **THEN** Tinymist schedules recompilation for any project that depended on that path
- **AND** the change is not skipped solely because no open-buffer contents changed
