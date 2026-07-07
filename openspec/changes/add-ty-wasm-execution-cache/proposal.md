## Why

The bytecode model needs an execution strategy that is fast, incremental, and safe under recursive dependencies. Shared blocking caches such as `OnceLock` can stall worker threads when type computations depend on each other, so execution must separate local running state from globally reusable results.

## What Changes

- Add a Rust interpreter backend for type bytecode.
- Add a Wasmer-backed execution backend behind an experimental feature flag.
- Add non-blocking local execution state for closures and calls.
- Add global caches only for completed bytecode programs, closure calls, and quoted results.
- Add instrumentation for cache hits, residualized cycles, VM steps, and Wasmer compile/execute time.

## Capabilities

### New Capabilities
- `ty-wasm-execution-cache`: Defines bytecode execution backends, non-blocking cycle handling, and cache behavior for type VM execution.

### Modified Capabilities

## Impact

- Affects new type VM modules and the type checker integration points that request evaluation.
- May add optional dependencies for Wasmer under a disabled-by-default feature.
- Introduces performance counters used by package-scale validation.
