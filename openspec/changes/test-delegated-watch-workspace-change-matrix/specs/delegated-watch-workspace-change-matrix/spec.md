## ADDED Requirements

### Requirement: Delegated watch client bookkeeping is covered
Tinymist SHALL test client-side delegated watch bookkeeping for watched paths, sync reads, stale-read suppression, and delete/unwatch handling.

#### Scenario: Sync watch request reads newly watched paths
- **WHEN** the server sends a `tinymist/fs/watch` request with inserted paths
- **THEN** the client-side watch layer MUST add those paths to the watched set
- **AND** it MUST read those files and send a `tinymist/fsChange` request with `isSync = true`
- **AND** successful reads and read errors MUST be represented in the insert payload

#### Scenario: Unwatch removes paths and suppresses stale reads
- **WHEN** the server sends a `tinymist/fs/watch` request with removed paths
- **THEN** the client-side watch layer MUST remove those paths from the watched set
- **AND** later filesystem events or delayed reads for those paths MUST NOT send stale inserts

#### Scenario: Read clock prevents older results from overwriting newer results
- **WHEN** multiple client-side reads race for the same watched URI
- **THEN** the delegated watch layer MUST send only the newest accepted result for that URI
- **AND** older read results MUST be ignored

### Requirement: Delegated fsChange requests normalize to workspace changes
Tinymist SHALL test server handling of `tinymist/fsChange` requests for delegated client watch events.

#### Scenario: Insert and remove payloads update server workspace state
- **WHEN** the server receives a `tinymist/fsChange` request with inserted and removed files
- **THEN** it MUST normalize the request into Tinymist filesystem changes
- **AND** affected source lookups and compile state MUST reflect the inserted, removed, or read-error observations

#### Scenario: Rename-like delegated changes retire old paths
- **WHEN** a delegated watcher represents a rename as old-path remove or read-error plus new-path insert
- **THEN** the old path MUST be retired from active server state
- **AND** the new path MUST become available only when the current workspace references it

#### Scenario: Sync and non-sync delegated changes are distinct
- **WHEN** delegated watch changes are sent as initial sync reads or later filesystem events
- **THEN** the server MUST preserve the `isSync` distinction when applying filesystem changes
- **AND** downstream compile scheduling MUST obey existing sync handling rules

### Requirement: Delegated ingress is mapped to modeled operation rows
Tinymist SHALL map delegated client watch behavior to the shared normalized operation model.

#### Scenario: Supported delegated rows are explicit
- **WHEN** delegated watch matrix coverage is implemented
- **THEN** it MUST identify which `O01..O20` rows are supported through delegated client watch ingress
- **AND** create, content update, remove, rename-as-delete/create, read-error recovery, dependency membership change, directory changes, and mixed-batch rows MUST have explicit expectations

#### Scenario: LSP-visible smoke checks use normalized server state
- **WHEN** a delegated watch row changes an entry, dependency, or diagnostic state
- **THEN** at least one representative tinymist-level LSP or diagnostics assertion MUST confirm that the server observes the normalized workspace change
- **AND** detailed per-API response transition coverage MAY be delegated to `tinymist-lsp-workspace-change-matrix`
