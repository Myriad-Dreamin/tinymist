## 1. Build the tinymist LSP integration harness

- [ ] 1.1 Add test-only helpers to construct `ServerState` with a deterministic mock workspace and project state.
- [ ] 1.2 Add helpers to apply normalized workspace changes, memory events, and assisted rename edits through tinymist state.
- [ ] 1.3 Add helpers to invoke selected LSP request handlers and collect diagnostics/semantic-token state without bypassing tinymist.

## 2. Define the LSP API response matrix

- [ ] 2.1 Map `O01..O20` to API sensitivity groups: graph-dependent, source-local, diagnostics, semantic tokens, rename assistance, and shadow-open flows.
- [ ] 2.2 Select representative APIs per row and document rows represented by the same assertion.
- [ ] 2.3 Define before/after fixtures with stable symbols, imports, labels, references, and diagnostics.

## 3. Add graph-dependent API transition tests

- [ ] 3.1 Test hover/definition/references/completion responses after create, edit, remove, rename, directory rename, and mixed-batch rows.
- [ ] 3.2 Test stale-reference rename rows do not return locations from retired old paths.
- [ ] 3.3 Test updated-reference rename rows return locations and completions from new paths.
- [ ] 3.4 Test workspace-symbol responses refresh after dependency and directory-prefix changes.

## 4. Add source-local, semantic-token, diagnostic, and shadow tests

- [ ] 4.1 Test document-symbol and semantic-token full responses after current-document content changes and atomic replacements.
- [ ] 4.2 Test semantic-token delta result ids after edits, renames, and shadow filesystem races.
- [ ] 4.3 Test diagnostics publish and clear after remove, read-error, recovery, stale rename, updated rename, and mixed-batch rows.
- [ ] 4.4 Test shadow-open files use memory content while open and filesystem content after close.

## 5. Validate

- [ ] 5.1 Run focused `cargo test -p tinymist` coverage for the new integration tests.
- [ ] 5.2 Run any required supporting `tinymist-project` tests for harness helpers.
- [ ] 5.3 Run `cargo fmt --check --all`.
