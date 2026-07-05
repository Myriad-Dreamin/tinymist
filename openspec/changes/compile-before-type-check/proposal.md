## Why

The current PR's deferred inference model improves precision but still leaves deduce and infer as visibly separate mechanisms. We need the checker to compile expressions first, use bytecode evaluation as the deduce engine, and then run compatibility checking over the evaluated semantic types.

## What Changes

- Replace the current deduce path with compile-to-bytecode followed by VM evaluation.
- Keep a check phase that applies existing compatibility and warning logic after semantic results are available.
- Preserve `precise_sig_of_def` as the public query for docs/signature consumers.
- Avoid exposing shallow signatures through documentation APIs.
- Retain snapshots as the primary correctness signal for this migration.

## Capabilities

### New Capabilities
- `compile-before-type-check`: Defines the checker pipeline that compiles syntax before checking and uses type VM evaluation for deduced results.

### Modified Capabilities

## Impact

- Affects the current deferred type-checker PR and its follow-up snapshot PR.
- Reworks `crates/tinymist-query/src/analysis/tyck.rs`, `tyck/syntax.rs`, and `tyck/apply.rs`.
- Requires careful snapshot review for stronger/weaker typings.
