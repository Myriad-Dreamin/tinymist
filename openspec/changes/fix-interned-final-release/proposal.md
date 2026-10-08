Authored with Codex (GPT-6).

## Why

`Interned::drop` checks the Arc owner count before its field is automatically destroyed. With a map owner and two caller handles, both callers can observe three owners, skip cleanup, and then decrement the count. This leaves a map-only allocation retained until an equal value is interned and released again.

The existing contract intends to remove a value when its final caller releases it. Count inspection and caller decrement need a shared synchronization protocol to uphold that contract during concurrent releases.

## What Changes

- Serialize caller count checks and Arc decrements with a release mutex per interned type.
- Recheck final removal while holding the map shard and release mutex, then remove the canonical allocation by pointer identity.
- Move the owned Arc into a local at the start of Drop while preserving pointer and nested-option layouts.
- Keep payload destruction outside interner locks and preserve the existing hash-callback environment.
- Add concurrent release and reinterning, recursive destruction, callback, collision, and representation regressions.

## Capabilities

### New Capabilities

- `interned-value-lifetime`: Specifies canonical ownership and final-caller cleanup during concurrent releases.

### Modified Capabilities

- None. There is no existing accepted interner-lifetime capability.

## Impact

- Implementation and unit tests are confined to `crates/tinymist-analysis/src/adt/interner.rs`.
- Public APIs, LSP configuration, dependency pins, and release metadata are unchanged.
- A per-type mutex adds contention across distinct allocations of the same type; this is an explicit correctness/throughput tradeoff.
- This repairs the retention race. It makes no claim about all sources of language-server memory growth.
