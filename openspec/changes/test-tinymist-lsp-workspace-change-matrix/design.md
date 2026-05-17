## Context

Most language-intelligence APIs are implemented in `tinymist-query`, but tinymist owns the server state that chooses the current project, applies filesystem and memory events, manages shadow-open files, tracks semantic token result ids, and schedules diagnostics. A query snapshot can be correct in isolation while the LSP server still answers with stale state after a workspace change.

The new tests should therefore live in `crates/tinymist` as class-integration tests: they exercise tinymist workspace state and LSP request handlers together, while relying on existing query-level tests for detailed API semantics.

## Goals / Non-Goals

**Goals:**
- Prove LSP API responses change correctly after modeled workspace changes.
- Cover response transitions rather than every detailed API feature.
- Use the `O01..O20` operation model to select representative workspace changes.
- Distinguish filesystem changes from text-sync shadow overlays and assisted rename flows.
- Confirm old paths, stale result ids, and old diagnostics are not reused after path retirement.

**Non-Goals:**
- Re-testing every hover/definition/completion semantic case already covered in `tinymist-query`.
- Testing raw watcher translation or VS Code delegated watcher transport.
- Testing editor UI behavior.
- Fixing stale-response bugs in this proposal unless small test harness work exposes a trivial correction.

## Decisions

### 1. Place tests in `crates/tinymist`, not `crates/tinymist-query`

The target is tinymist's integration boundary: workspace changes enter server/project state and later LSP handlers query the active snapshot. Tests may call handler methods directly or use a light LSP driver, but they must not bypass tinymist state by constructing query snapshots directly.

Alternative considered: add more snapshot tests to `tinymist-query`. Rejected because those tests cannot prove server-state refresh, memory overlay ordering, diagnostics publication, or semantic token result-id invalidation after workspace changes.

### 2. Test response transitions over representative fixtures

Each API should use small fixtures with stable expected response shape before and after a workspace change. For example, definition/references can assert URI movement after rename, hover/completion can assert a symbol appears or disappears after dependency updates, and document symbols can assert current file content rather than full query semantics.

Alternative considered: produce large snapshots for every operation/API pair. Rejected because it would be noisy and duplicate query-crate coverage.

### 3. Classify APIs by workspace-change sensitivity

The matrix should group APIs by the state they depend on:

- source-local APIs: document symbol, semantic tokens, folding/selection where useful
- graph-dependent APIs: hover, definition/declaration, references, completion, workspace symbol
- diagnostic APIs: publish diagnostics after compile/read-error changes
- rename-related APIs: `workspace/willRenameFiles`, prepare/rename when workspace paths move

The implementation should choose representative APIs per group for every row, and expand only rows that have distinct LSP-visible obligations.

Alternative considered: require every API for every row. Rejected because many rows have identical API obligations once the workspace snapshot is correct.

### 4. Treat semantic tokens and diagnostics as stateful APIs

Semantic token delta tests must ensure old result ids cannot resurrect old file contents after edits, renames, or shadow filesystem races. Diagnostic tests must assert publish/clear transitions after missing files, read errors, recovery, and updated imports.

Alternative considered: only test stateless request/response APIs. Rejected because stale workspace-change bugs often surface through diagnostics and semantic token caches.

## Risks / Trade-offs

- [Tinymist LSP harness could become heavy] -> Build minimal test-only helpers that apply workspace changes and invoke handlers without a full editor process.
- [Tests duplicate query snapshots] -> Keep assertions focused on response transitions, URIs, result ids, diagnostics lifecycle, and stale-path absence.
- [Async compile scheduling can be flaky] -> Use deterministic project/compiler helpers and explicit wait points for diagnostic publication.
- [Matrix scope can explode] -> Use row ids and API sensitivity groups; document rows represented by the same integration assertion.

## Migration Plan

1. Add a tinymist LSP class-integration test harness with mock workspace setup, workspace-change application, and request invocation.
2. Define an API sensitivity matrix over `O01..O20`.
3. Add response-transition tests for graph-dependent APIs, source-local APIs, semantic tokens, diagnostics, and rename assistance.
4. Document rows that share equivalent LSP obligations.
5. Validate with focused `cargo test -p tinymist` commands and formatting checks.

## Open Questions

- Whether the first implementation should use direct `ServerState` handler calls or drive requests through `sync-lsp`'s typed test transport.
- Which diagnostic wait mechanism should be standardized for deterministic test assertions.
