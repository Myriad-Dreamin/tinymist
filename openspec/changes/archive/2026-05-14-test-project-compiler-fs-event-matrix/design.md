## Context

Tinymist already has reusable mock layers for VFS, world, and project compiler tests. Those mocks can create in-memory workspaces, mutate files, and route `FileChangeSet` values through the same `FilesystemEvent` entry point used by the runtime.

The gap is not the lack of a mock workspace. The gap is that project compiler tests do not yet define a complete event taxonomy for user-level file operations. This makes rename, remove, failed read, and save-like sequences easy to discuss but hard to validate consistently.

This change is a precursor to bug fixes such as source rename cache invalidation. It should make the project compiler behavior observable before changing the behavior itself.

## Goals / Non-Goals

**Goals:**
- Define the filesystem event dimensions that project compiler tests must cover.
- Map common user operations to runtime-facing `FilesystemEvent` and `FileChangeSet` shapes.
- Drive those shapes through `ProjectCompiler` using existing mock workspace APIs.
- Assert compiler-visible outcomes: compile reasons, dependency sync messages, VFS freshness, diagnostics or compile results, and harmless-change decisions.

**Non-Goals:**
- Testing `NotifyActor` internals. That belongs to `test-notify-actor-fs-event-matrix`.
- Simulating every backend-specific `notify-rs` quirk at this layer.
- Changing runtime invalidation behavior or fixing rename/cache bugs in this proposal.
- Testing VS Code client-side watch transport behavior.

## Decisions

### 1. Define the project compiler matrix in terms of `FilesystemEvent`

The project compiler consumes `FilesystemEvent`, not raw `notify::Event`. This proposal should define the matrix at that boundary:

| Dimension | Values to cover |
| --- | --- |
| Event variant | `Update`, `UpstreamUpdate` |
| Sync flag | `is_sync = true`, `is_sync = false` |
| Insert payload | successful non-empty content, successful empty content, read error snapshot |
| Remove payload | no removes, one removed path, multiple removed paths |
| Path relation | entry file, imported dependency, previously depended path, newly created dependency, unrelated file |
| Batch shape | insert-only, remove-only, remove plus insert, multi-file batch, empty changeset |
| Sequence shape | one-step edit, delete then recreate, rename old plus new, old path retired then entry edited, failed read then recovery |

The implementation should document unreachable or intentionally unsupported combinations instead of silently omitting them.

Alternative considered:
- Build project compiler tests from raw `notify::Event` values. Rejected because it would mix watcher translation with compiler behavior and duplicate the notify actor proposal.

### 2. Treat user operations as named fixtures over the event matrix

The matrix should be easier to maintain if tests are organized by user operation:

- create a dependency
- edit an existing entry or dependency
- remove an entry or dependency
- rename a dependency with import text updated
- rename a dependency without import text updated
- atomic save style remove/error/empty followed by restored content
- failed client read followed by successful read
- unrelated file churn
- initial sync and follow-up non-sync updates

Each operation can expand into one or more `FileChangeSet` shapes. The same named operation can later be reused by notify actor tests, but this proposal should only assert what happens after the compiler receives the runtime-facing event.

Alternative considered:
- Hand-write isolated regression tests for only the known rename bug. Rejected because the user goal is a comprehensive FS event matrix before bug fixing.

### 3. Use `tinymist-project` mock compiler tests as the primary location

Most tests should live close to `ProjectCompiler`, using:

- `MockWorkspace` for deterministic file state
- `MockWorkspaceWorldExt` for compiler universe construction
- `MockProjectBuilderExt` and `MockProjectChangeExt` for driving `Interrupt::Fs`
- direct inspection of notify receiver messages for dependency sync coverage

World-level tests can remain for lower-level VFS freshness checks, but the main assertion target should be the project compiler's behavior.

Alternative considered:
- Build a full LSP `ServerState` and test via `tinymist/fsChange`. Rejected for this proposal because it adds transport, editor scheduling, and config setup that are not necessary to validate compiler behavior.

### 4. Assert outcomes, not only that events are accepted

Tests should verify the result that matters to users and downstream compiler consumers:

- whether `reason.by_fs_events` or `reason.by_mem_events` is set
- whether `NotifyMessage::SyncDependency` reflects current dependencies after compile
- whether a removed or renamed dependency path is no longer read from stale VFS state
- whether diagnostics or compile results reflect the current workspace
- whether unrelated VFS changes are allowed to remain harmless
- whether sync events obey `ignore_first_sync` expectations

Alternative considered:
- Only assert that `ProjectCompiler::process` does not panic. Rejected because that would not protect cache, dependency, or compile scheduling semantics.

## Risks / Trade-offs

- [Matrix grows too large] -> Mitigate by defining dimensions, named user-operation fixtures, and explicit pruning rules for unreachable combinations.
- [Tests overfit current bugs] -> Mitigate by asserting compiler-visible contracts rather than implementation details such as private cache shape.
- [Mock behavior diverges from real watcher behavior] -> Mitigate by keeping raw watcher translation in the separate notify actor proposal and using this proposal only at the `FilesystemEvent` boundary.
- [Full compile tests are slower than syntax-only tests] -> Mitigate by using syntax-only only when dependency behavior is irrelevant, and full compile only for import/dependency assertions.

## Migration Plan

1. Add the project compiler filesystem event matrix and named user-operation fixtures.
2. Add focused tests that drive each matrix row through mock-backed `ProjectCompiler`.
3. Add assertions for dependency sync, compile reasons, diagnostics or compile freshness, and harmless VFS behavior.
4. Run focused tests for `tinymist-vfs`, `tinymist-world`, and `tinymist-project` as needed.
5. Rollback is a straightforward revert because this proposal only adds tests and test helpers.

## Open Questions

- Whether harmless VFS decision logic should be extracted into a small helper so it can be tested without constructing `tinymist` LSP state.
- Whether multi-project dedicated task coverage should be part of the first implementation pass or added after primary-project coverage is complete.
