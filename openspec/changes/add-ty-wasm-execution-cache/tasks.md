## 1. Interpreter Backend

- [x] 1.1 Add the backend trait and Rust interpreter implementation.
- [x] 1.2 Implement stack, frame, environment, closure, and meta store handling for supported bytecode.
- [x] 1.3 Add recursion handling that converts local `Running` calls into neutral residuals.

## 2. Caches

- [x] 2.1 Add local closure-call state maps for non-blocking demand-driven execution.
- [x] 2.2 Add global completed-result caches for programs, closure calls, and quoted values.
- [x] 2.3 Add cache invalidation keys for source revision, captured environment, argument shape, context, and meta epoch.

## 3. Wasmer Backend

- [x] 3.1 Add optional Wasmer dependency and feature flag.
- [x] 3.2 Implement wasm module instantiation through the handle-based host ABI.
- [x] 3.3 Add interpreter-vs-Wasmer equivalence tests for the supported bytecode subset.

## 4. Metrics

- [x] 4.1 Add VM counters for steps, cache hits, cache misses, and residualized cycles.
- [x] 4.2 Add Wasmer compile/run timing counters behind the experimental backend.
- [x] 4.3 Surface metrics in package scan output under `target/`.
