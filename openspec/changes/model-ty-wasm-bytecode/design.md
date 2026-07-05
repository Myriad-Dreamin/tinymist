## Context

The current checker directly maps syntax to `Ty` and uses deferred `Ty::Apply`, generated variables, and post-processing to infer function result types. That model is too close to syntax and too far from evaluation: calls, selection, conditionals, and binary operations are repeatedly interpreted through ad hoc code paths.

Typst's concrete evaluator already demonstrates the shape we want: closures capture scopes, calls run in a new VM, imports are route-aware, and expensive computations are cached. For type checking, however, the value domain must support metas and neutral residuals, so a tiny type VM is needed instead of reusing Typst's evaluator.

## Goals / Non-Goals

**Goals:**
- Define a small bytecode instruction set for type-level evaluation.
- Define semantic values that can represent computed types, closures, metas, and stuck neutral terms.
- Provide a deterministic quote step from semantic values back to existing `Ty`.
- Provide an emitter contract that can compile bytecode to WebAssembly later.

**Non-Goals:**
- Execute the bytecode in Wasmer.
- Replace the existing checker behavior.
- Type-check every Typst expression kind in the first bytecode model.
- Expose semantic values in public analysis APIs.

## Decisions

- Use `Ty` as the external format and semantic values as the VM-internal format. This avoids snapshot/API churn while allowing NbE-style execution internally.
- Model functions as bytecode closures with captured type scopes and a return meta. This mirrors Typst closure evaluation while allowing recursive calls to residualize instead of block.
- Model stuck operations as neutral values rather than immediate `Ty::Apply` terms. Quoting can still produce `Ty::Apply`, but the evaluator retains more structure while running.
- Keep the first WebAssembly emitter host-driven. Wasm code should operate on handles and call host functions for type algebra rather than owning Rust `Ty` layouts.
- Keep bytecode stable enough for tests but not a public compatibility boundary.

## Risks / Trade-offs

- [Risk] The bytecode model may grow too much if it tries to encode all Typst semantics. -> Mitigation: only encode operations needed by type deduction and leave concrete Typst eval separate.
- [Risk] Quoting semantic values can become expensive. -> Mitigation: design quote cache keys around semantic value IDs and quote mode from the start.
- [Risk] Wasm emission could constrain the interpreter too early. -> Mitigation: define a wasm-friendly instruction set but validate semantics first in Rust.
- [Risk] Existing `Ty` operations may duplicate semantic operations. -> Mitigation: migrate only after snapshots show equivalent or stronger results.
