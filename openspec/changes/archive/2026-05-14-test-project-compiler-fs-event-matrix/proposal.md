## Why

Tinymist currently has mock-backed runtime helpers, but project compiler coverage does not systematically enumerate the filesystem event combinations produced by common user operations. Before fixing rename/cache bugs, we need a deterministic matrix that shows how `ProjectCompiler` should behave after it receives each runtime-facing filesystem event shape.

## What Changes

- Define a user-operation filesystem event matrix for project runtime tests, covering creates, edits, deletes, renames, save-like remove/recreate sequences, transient empty writes, failed reads, sync events, non-sync events, and follow-up edits after dependency changes.
- Add focused project compiler tests that drive those combinations through existing mock workspace and `FilesystemEvent`/`FileChangeSet` helpers.
- Assert user-visible compiler outcomes, including compile reasons, dependency refresh, diagnostics freshness, stale-path retirement, and whether harmless VFS changes are skipped or recompiled.
- Keep this as test coverage and harness work only; it does not fix runtime cache invalidation behavior by itself.

## Capabilities

### New Capabilities
- `project-compiler-fs-event-matrix`: Defines deterministic project compiler coverage for runtime-facing filesystem event combinations derived from user file operations.

### Modified Capabilities
- None.

## Impact

- Affected Rust areas: `crates/tinymist-project`, `crates/tinymist-vfs`, `crates/tinymist-world`, and aggregate mock support in `crates/tinymist-tests`.
- Uses existing mock workspace APIs such as create, update, remove, rename, and `apply_as_fs_to_project`.
- May add small test-only helpers to inspect dependency sync messages, compile reasons, and compiler outputs without constructing a full LSP server.
- No production behavior, public runtime API, dependency pin, or generated documentation change is intended.
