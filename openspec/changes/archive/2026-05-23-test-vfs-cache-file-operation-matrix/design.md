## Context

The operation model in `docs/tinymist/dev/notify-rs-file-operation-decomposition.typ` partitions correctness-relevant user file operations into `O01..O20`. Tinymist already has mock workspace support and project compiler filesystem event coverage, but cache-specific behavior can still be hidden behind a later compile result. Rename and remove bugs are especially easy to miss when an old path remains readable from a cache even though the current workspace no longer contains it.

This change isolates cache correctness below the LSP layer and above raw watcher translation. It should consume normalized mock workspace changes, not `notify::Event` values.

## Goals / Non-Goals

**Goals:**
- Encode the `U_cov` operation classes as VFS/cache test cases.
- Assert VFS path state, source cache invalidation, file id/path map behavior, and read-error snapshots after each class.
- Assert compile-cache freshness for rows that affect entries, dependencies, assets, directory-prefix paths, shadow-open paths, or mixed batches.
- Document any `O01..O20` row that is represented by another row at the VFS/cache boundary.

**Non-Goals:**
- Testing raw `notify-rs` event translation. That belongs to notify actor coverage.
- Testing LSP API response transitions. That belongs to tinymist-level LSP class integration coverage.
- Fixing cache invalidation bugs in this proposal. This proposal first makes stale state observable.
- Replacing `project-compiler-fs-event-matrix`.

## Decisions

### 1. Test at the normalized VFS/mock workspace boundary

The matrix should use mock workspace operations and normalized change shapes, not host filesystem operations. This keeps the test deterministic and lets it assert exact postconditions such as `source(old)` missing, `source(new)` current, read errors replacing old snapshots, and renamed paths not sharing stale parsed source state.

Alternative considered: drive all cases through `NotifyActor`. Rejected because watcher timing and backend coalescing would obscure cache-specific failures.

### 2. Keep the row vocabulary aligned with `O01..O20`

Each test case should carry the operation row id from the model. Rows may share helper fixtures, but test output and failure messages should preserve the row id so gaps can be audited against the Typst document and Z3 proof.

Alternative considered: define a separate cache-only taxonomy. Rejected because it would make no-duplicate/no-omission reasoning drift from the shared model.

### 3. Assert cache state before and after compile

For affected entry/dependency rows, tests should first assert VFS/world state immediately after mutation, then drive a compile or query snapshot when needed to assert compile-cache freshness. This separates "VFS invalidated correctly" from "compiler consumed the invalidation correctly".

Alternative considered: only assert final compiler diagnostics. Rejected because stale VFS state can be masked by unrelated compiler behavior.

### 4. Treat unrelated and retained-inactive paths as explicit dimensions

Rows that include unrelated files or retained inactive dependencies should assert that harmless churn stays harmless while late events for retired paths do not re-activate stale content. This prevents the matrix from becoming only a "changed dependency" suite.

Alternative considered: omit unrelated variants. Rejected because harmless-change decisions are part of the correctness surface.

## Risks / Trade-offs

- [Matrix size grows quickly] -> Use the `O01..O20` ids as coverage anchors and group variants only when the VFS/cache postcondition is identical.
- [Tests overfit private cache internals] -> Prefer observable helpers such as source lookup, path resolution, compile snapshots, and diagnostics; add private inspection only when no observable surface exists.
- [Mock workspace semantics drift from runtime] -> Keep raw watcher translation in separate proposals and test only normalized `FileChangeSet`/workspace operations here.
- [Bug-fixing pressure during test authoring] -> Land failing tests behind targeted tasks or mark known failures clearly until the corresponding fix proposal is applied.

## Migration Plan

1. Add matrix fixtures that map `O01..O20` to mock workspace mutations and normalized change shapes.
2. Add VFS/world assertions for path state, read results, file ids, and source freshness.
3. Add compile-cache assertions for affected rows that require a compile snapshot.
4. Document grouped, redundant, or deferred rows in the test matrix.
5. Validate with focused `tinymist-vfs`, `tinymist-world`, and `tinymist-project` tests.

## Open Questions

- Whether file-id/path-map inspection should be exposed through test-only helpers or asserted indirectly through `source` and compile snapshots.
- Whether symlink/link target rows can be fully represented in the in-memory mock layer or need platform-specific ignored tests later.
