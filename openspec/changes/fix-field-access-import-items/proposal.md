## Why

Import statements such as `#import theoretic.presets.corners: theorem, lemma` are valid Typst and eventually work once tinymist falls back to dynamic import analysis, but tinymist's static import analysis currently treats the field-access source as unsupported for non-wildcard item imports. That gap produces the warning reported in issue `#2410`, leaves editor-side import scope analysis incomplete, and can force the slower full-compilation fallback called out in the issue comments.

## What Changes

- Resolve module-valued field-access import sources during static analysis for item imports such as `#import foo.bar: baz`.
- Populate imported bindings from those sources through the same import declaration path used for string and alias-based module imports.
- Keep unsupported non-module source expressions failing closed instead of manufacturing bindings from unresolved expressions.
- Add regression coverage for direct item imports from field-access sources, nested import item paths, and existing wildcard field-access imports.

## Capabilities

### New Capabilities
- `field-access-import-items`: Treat module-valued field-access expressions as valid static sources for non-wildcard import item lists.

### Modified Capabilities
- None.

## Impact

- `crates/tinymist-query/src/syntax/expr.rs`
- `crates/tinymist-query/src/analysis/global.rs` and related import-resolution helpers
- Import-related fixtures under `crates/tinymist-query/src/fixtures/`
