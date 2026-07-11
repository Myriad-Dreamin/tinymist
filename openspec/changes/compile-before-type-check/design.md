## Context

The ongoing type-checker work introduced generated return variables, deferred calls, precise signature queries, and fixed-point inference. That validates the direction, but the implementation still spreads evaluation across syntax checking, deferred-body queues, call instantiation, and result normalization.

The new pipeline should make bytecode evaluation the deduce stage. Checking becomes a consumer of evaluated semantic values instead of a second way to discover the same information.

## Goals / Non-Goals

**Goals:**
- Compile syntax into type bytecode before deducing expression and binding types.
- Use the type VM to evaluate functions on demand when calls require their result.
- Run compatibility checks after or during evaluation without sacrificing precision.
- Preserve existing public analysis APIs and snapshot testing workflow.
- Avoid shared blocking `get_or_init` waits between worker threads.

**Non-Goals:**
- Introduce Wasmer execution in this PR.
- Complete all package-scale performance work in this PR.
- Remove every existing `Ty` simplification helper immediately.

## Decisions

- Treat bytecode evaluation as the deduce stage. Function definitions create closures and return metas; function calls force closures when possible.
- Keep checking logic as VM primitives or post-evaluation checks rather than duplicating expression traversal.
- Make `precise_sig_of_def` force the relevant closure result through the VM and quote the final signature.
- Represent cycles as neutral residuals, not as blocking waits or immediate `Any`.
- Resolve documentation annotations through existing flow semantic types before applying them as input contracts. A wholly unresolved annotation contributes no bound instead of a synthetic `Any` bound.
- Keep old checker paths behind tests during migration only if needed for diff analysis, not as a permanent dual implementation.

## Risks / Trade-offs

- [Risk] The migration can produce many snapshot diffs at once. -> Mitigation: add focused fixtures and classify diffs as stronger, weaker, or formatting-only before accepting.
- [Risk] Some existing checker warnings depend on traversal order. -> Mitigation: encode those checks as VM primitives with explicit once-only warning behavior.
- [Risk] Incomplete bytecode coverage can regress features. -> Mitigation: fall back only for unsupported syntax while logging coverage gaps in tests.
- [Risk] Precise signature queries can accidentally get shallow results. -> Mitigation: make the analysis query force the relevant closure result and test docs/signature paths.
