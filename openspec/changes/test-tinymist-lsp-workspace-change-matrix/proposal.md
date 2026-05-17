## Why

Tinymist-query snapshot tests validate individual language-intelligence algorithms, but they do not prove that tinymist's LSP-facing server state refreshes those API responses after workspace filesystem changes. The notify-rs operation model gives us a finite set of workspace-change classes; tinymist needs class-integration tests that prove LSP responses move from the old workspace state to the new one.

## What Changes

- Add tinymist-level "class integration" tests for LSP API responses after modeled workspace changes.
- Drive file operations through tinymist server/project state using normalized workspace changes, memory overlays, or `tinymist/fsChange`-equivalent inputs rather than calling tinymist-query directly.
- Cover representative LSP APIs whose responses depend on workspace state: hover, definition/declaration, references, completion, document symbols, semantic tokens full/delta, diagnostics publication, workspace symbols, and rename assistance where relevant.
- Assert response transitions across `O01..O20` operation classes: old URI disappears, new URI appears when referenced, diagnostics recover or appear, semantic token result ids do not resurrect stale files, and shadow-open state is deterministic.
- Keep per-API semantic correctness in `tinymist-query`; this proposal tests tinymist's workspace-change propagation to API responses.

## Capabilities

### New Capabilities
- `tinymist-lsp-workspace-change-matrix`: Defines tinymist-level LSP response transition coverage for modeled workspace changes.

### Modified Capabilities
- None.

## Impact

- Affected Rust areas: primarily `crates/tinymist`, with supporting use of `crates/tinymist-project`, `crates/tinymist-vfs`, and shared test fixtures.
- May add test-only server-state or LSP-harness helpers for applying workspace changes and invoking request handlers.
- Does not move LSP API behavior tests into `tinymist-query`; query crate tests remain responsible for per-API algorithmic correctness.
- No protocol, editor, dependency, or production behavior change is intended.
