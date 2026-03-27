## Why

Function-parameter rename in tinymist currently updates the declaration and in-body references but skips named argument labels at call sites such as `bubble(side: left)` and `bubble.with(side: left)`. That produces partial renames that can leave semantically stale call sites behind, and it blocks the roadmap item tinymist already advertises around named function argument references.

## What Changes

- Track named argument labels in direct calls as semantic references to the user-defined parameter they bind to.
- Track named argument labels supplied through `func.with(...)` as semantic references to the same parameter.
- Make `textDocument/references` and `textDocument/rename` include those call-site labels through the same reference lookup path.
- Add regression coverage for direct calls, `.with(...)` partial application, and unrelated same-name parameters that must remain untouched.

## Capabilities

### New Capabilities
- `named-argument-references`: Treat named function argument labels as semantic references to user-defined parameters for reference-based LSP features.

### Modified Capabilities
- None.

## Impact

- `crates/tinymist-query/src/references.rs`
- `crates/tinymist-query/src/rename.rs`
- `crates/tinymist-query/src/analysis/call.rs` and related semantic lookup used to resolve call parameters
- Rename/reference fixtures under `crates/tinymist-query/src/fixtures/`
