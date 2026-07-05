## Context

The type checker has previously hit timeouts when computations waited on each other through nested cache initialization. The type VM must support demand-driven function execution, but a running recursive dependency must produce a neutral residual rather than wait for another worker.

The Wasmer backend is a long-term acceleration path. Most functions are likely cold, so the Rust interpreter must remain the default until profiling proves Wasmer is beneficial for hot programs.

## Goals / Non-Goals

**Goals:**
- Run type bytecode through a Rust interpreter.
- Define a backend trait so Wasmer execution can be added without changing checker semantics.
- Add non-blocking local `Fresh/Running/Done/Stuck` state for closure calls.
- Cache completed results globally while never globally blocking on running type computations.
- Record enough metrics to decide when Wasmer should be used.

**Non-Goals:**
- Make Wasmer the default backend.
- Move Rust type algebra into wasm memory.
- Guarantee cache reuse across incompatible source revisions or meta epochs.

## Decisions

- Use the Rust interpreter as the source of truth. Wasmer must be checked against interpreter results before being trusted for a program subset.
- Use local call states for cycle detection. `Running` means return a neutral residual; it never means wait.
- Use global caches only for completed results. This prevents worker-to-worker deadlocks while still sharing work.
- Key closure-call caches by closure prototype, captured environment key, argument shape, context key, and meta epoch.
- Cache quote results separately from evaluation results because quoting can dominate runtime once semantic values are shared.

## Risks / Trade-offs

- [Risk] Wasmer startup and compilation overhead may exceed interpreter cost. -> Mitigation: keep Wasmer feature-gated and add hotness thresholds.
- [Risk] Cache keys may be too precise and miss reuse. -> Mitigation: start conservative, then relax only with package-scan evidence.
- [Risk] Local `Running` residuals could weaken results. -> Mitigation: add snapshots for recursive and non-recursive demand-driven calls.
- [Risk] Metrics could add overhead. -> Mitigation: keep detailed counters behind existing tracing or debug configuration.
