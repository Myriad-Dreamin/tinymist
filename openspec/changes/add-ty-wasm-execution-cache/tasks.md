## 1. Interpreter Backend

- [ ] 1.1 Add the backend trait and Rust interpreter implementation.
- [ ] 1.2 Implement stack, frame, environment, closure, and meta store handling for supported bytecode.
- [ ] 1.3 Add recursion handling that converts local `Running` calls into neutral residuals.

## 2. Caches

- [ ] 2.1 Add local closure-call state maps for non-blocking demand-driven execution.
- [ ] 2.2 Add global completed-result caches for programs, closure calls, and quoted values.
- [ ] 2.3 Add cache invalidation keys for source revision, captured environment, argument shape, context, and meta epoch.

## 3. Wasmer Backend

- [ ] 3.1 Add optional Wasmer dependency and feature flag.
- [ ] 3.2 Implement wasm module instantiation through the handle-based host ABI.
- [ ] 3.3 Add interpreter-vs-Wasmer equivalence tests for the supported bytecode subset.

## 4. Metrics

- [ ] 4.1 Add VM counters for steps, cache hits, cache misses, and residualized cycles.
- [ ] 4.2 Add Wasmer compile/run timing counters behind the experimental backend.
- [ ] 4.3 Surface metrics in package scan output under `target/`.
