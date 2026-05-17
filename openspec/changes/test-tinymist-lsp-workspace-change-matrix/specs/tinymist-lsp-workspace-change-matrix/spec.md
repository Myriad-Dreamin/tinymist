## ADDED Requirements

### Requirement: LSP workspace-change matrix is tested at the tinymist integration layer
Tinymist SHALL define LSP API response transition tests at the tinymist server-state layer for modeled workspace changes.

#### Scenario: Tests do not bypass tinymist server state
- **WHEN** an LSP workspace-change matrix test invokes a language feature
- **THEN** it MUST apply the workspace change through tinymist project/server state before invoking the LSP handler
- **AND** it MUST NOT construct a tinymist-query snapshot directly as the primary test path

#### Scenario: Query-crate correctness remains separate
- **WHEN** a test asserts an LSP API response after a workspace change
- **THEN** the assertion MUST focus on response transition, current URI, stale URI absence, result-id freshness, or diagnostics lifecycle
- **AND** it MUST NOT attempt to replace tinymist-query's per-API semantic snapshot coverage

### Requirement: LSP responses reflect modeled workspace changes
Tinymist SHALL assert that LSP API responses change according to the current workspace state after normalized file-operation rows.

#### Scenario: Graph-dependent responses follow dependency changes
- **WHEN** a modeled operation creates, removes, renames, moves, or updates a source dependency
- **THEN** hover, definition/declaration, references, completion, or workspace-symbol responses selected for that row MUST reflect the current dependency graph
- **AND** responses MUST NOT point at retired paths except when reporting a missing import or unavailable target

#### Scenario: Source-local responses use current file contents
- **WHEN** a modeled operation changes the current document's source bytes or shadow-open memory overlay
- **THEN** document-symbol and semantic-token responses selected for that row MUST reflect the current source
- **AND** old semantic token delta result ids MUST NOT resurrect stale content after workspace changes

#### Scenario: Diagnostics transition with missing and recovered files
- **WHEN** a modeled operation removes, read-errors, restores, or updates a dependency path
- **THEN** diagnostics publication MUST transition to the missing/read-error state or recover to the clean state according to the final workspace
- **AND** old diagnostics for retired paths MUST be cleared or replaced according to tinymist's diagnostic ownership rules

### Requirement: LSP rename and shadow workflows are covered as workspace-change classes
Tinymist SHALL include rename assistance and shadow-open filesystem races in the LSP workspace-change matrix.

#### Scenario: Assisted and unassisted renames have distinct assertions
- **WHEN** a file or directory rename is assisted by `workspace/willRenameFiles`
- **THEN** the matrix MUST assert the workspace edit response for import updates
- **AND** after the workspace change is applied, selected LSP responses MUST refer to the new path
- **AND** unassisted rename rows MUST still assert that stale old paths are not served from cache

#### Scenario: Shadow-open files resolve deterministically
- **WHEN** a file is open in memory while the filesystem changes, moves, or deletes the same path
- **THEN** selected LSP responses MUST use the active memory overlay while it is open
- **AND** after close, selected LSP responses MUST reveal the current filesystem state rather than stale overlay or stale filesystem cache
