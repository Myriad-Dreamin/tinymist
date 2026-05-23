## Why

The notify-rs file-operation decomposition now gives Tinymist a finite `U_cov = O01..O20` model for correctness-relevant user file operations, but the lower VFS/world/cache layers still lack direct state assertions for every class. Existing project compiler matrix coverage proves many compiler-visible outcomes, yet it does not isolate VFS path state, source cache retirement, read-error snapshots, and compile-cache freshness as first-class contracts.

## What Changes

- Add a VFS/cache-focused test matrix derived from `docs/tinymist/dev/notify-rs-file-operation-decomposition.typ`.
- Cover the `O01..O20` operation classes at the normalized `FileChangeSet`/mock workspace boundary, grouping only rows that have identical VFS/cache obligations.
- Assert VFS state directly after creates, edits, deletes, renames, directory-prefix changes, read-error transitions, symlink-like observable path changes, shadow filesystem races, and mixed batches.
- Assert compile-cache state where VFS changes affect `ProjectCompiler` freshness: old paths must retire, new paths must be readable when referenced, and unrelated churn must remain harmless.
- Keep this proposal test-only. It must not change production watcher policy or LSP handler behavior.

## Capabilities

### New Capabilities
- `vfs-cache-file-operation-matrix`: Defines deterministic VFS, world, and compile-cache state coverage for normalized user file-operation classes.

### Modified Capabilities
- None.

## Impact

- Affected Rust areas: `crates/tinymist-vfs`, `crates/tinymist-world`, `crates/tinymist-project`, and shared mock test helpers.
- Builds on `mock-vfs-world-testing` and complements, but does not replace, `project-compiler-fs-event-matrix`.
- May add test-only inspection helpers for source snapshots, file ids/path maps, cached parsed sources, dependency snapshots, and latest compilation freshness.
- No generated Markdown, editor integration, dependency pin, or public API change is intended.
