## Why

The current type checker mixes syntax traversal, deferred terms, and result inference, which makes scope-based deduce hard to reason about and hard to optimize. We need a compact execution model that can evaluate type operations uniformly and later emit the same program to WebAssembly.

## What Changes

- Introduce a type bytecode model for expressions relevant to type deduction.
- Define a semantic type value domain with meta variables, neutral residuals, closures, arguments, records, and quoted `Ty` output.
- Add a compiler from the existing expression/type-checker representation to bytecode programs.
- Add an experimental WebAssembly emitter for the bytecode without changing the default checker execution path.
- Keep `Ty` as the public representation used by snapshots, signatures, docs, and analysis APIs.

## Capabilities

### New Capabilities
- `ty-wasm-bytecode-model`: Defines the type-level bytecode, semantic values, closure model, neutral residuals, and WebAssembly emission contract.

### Modified Capabilities

## Impact

- Affects `crates/tinymist-query/src/analysis/tyck*` and may add a new internal bytecode module under `tinymist-query` or `tinymist-analysis`.
- Adds no runtime dependency for the default path in this phase.
- Establishes the intermediate representation required by later execution/cache and compile-before-check work.
