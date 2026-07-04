## Why

Citation completion currently assumes every bibliography key can be inserted as a label literal like `#cite(<key>)`. Issue `#2395` shows that BibTeX keys such as DBLP entries can contain characters that are not valid in angle-bracket label syntax, so accepting a completion inserts invalid Typst instead of a usable citation.

## What Changes

- Make citation completion choose a syntactically valid insertion form for each bibliography key.
- Keep using angle-bracket label syntax for bibliography keys that are valid Typst labels and for existing non-citation label completion flows.
- Use explicit `label("...")` syntax when completing bibliography keys that cannot be represented as `<...>` inside `#cite(...)`.
- Add regression coverage for explicit-label citation completions, including partial-key completion and title-backed completion items for bibliography entries.

## Capabilities

### New Capabilities
- `citation-key-completion-syntax`: Citation completions insert a valid `#cite(...)` argument for every bibliography key, including keys that require `label("...")` instead of angle-bracket label syntax.

### Modified Capabilities
- None.

## Impact

- `crates/tinymist-query/src/analysis/completion/typst_specific.rs`
- Citation-related completion fixtures and snapshots under `crates/tinymist-query/src/fixtures/completion/`
- Potential label-analysis metadata in `crates/tinymist-analysis/src/track_values.rs` if completion needs per-key insertion hints
