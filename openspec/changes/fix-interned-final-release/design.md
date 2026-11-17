Authored with Codex (GPT-6).

## Context

The interner stores a canonical `Arc<T>` in a sharded map and returns caller-owned clones. A count of two means the map and the releasing caller are the only owners. Previously, two releases could both inspect a count of three before either automatic field decrement, so neither removed the map entry.

The repair must preserve fast pointer-based identity, supported sized/unsized representations, and nested-option niches. Hashing and payload destruction can release other interned handles, so lock placement and unwind ordering are part of the design.

## Goals / Non-Goals

Goals:
- Reclaim the canonical map allocation after successful final caller release, including concurrent releases.
- Keep lookup/reinterning consistent with removal.
- Preserve representation and existing callback behavior.
- Keep final payload destruction outside interner locks.

Non-goals:
- Replace the interner, alter its public API, or introduce editor configuration.
- Change dependency pins or the query-history garbage-collection implementation.
- Establish long-running memory improvements or performance equivalence from bounded workloads.

## Decisions

### Serialize count inspection and caller decrement

Each `InternStorage<T>` has a release mutex. A nonfinal release inspects the count and explicitly drops its Arc while holding this mutex. Another caller and the map still retain the allocation, so this branch cannot run `T`'s destructor under the mutex.

The final branch releases the mutex before hashing. It then locks the map shard, acquires the release mutex, and rechecks the count. A concurrent intern may have added a caller while hashing or waiting for the shard. In that case, it unlocks the shard and decrements its Arc under the release protocol.

The acquisition order when both guards are needed is shard then release. The first count check never holds release while acquiring a shard.

### Own cleanup explicitly without losing the Arc niche

The private field is `ManuallyDrop<Arc<T>>`. `ManuallyDrop::take` executes once at the start of Drop, before fallible operations, and produces an ordinary local Arc. That local is cleaned up on normal returns and unwinding. The consumed private field is never read, dropped, or moved afterward.

Manual trait implementations only inspect live values; no derived implementation exposes the consumed field. Unlike an `Option<Arc<T>>` field, this representation preserves the null niche needed by `Option<Interned<T>>`. Tests cover sized handles and `str`, including their nested options.

### Remove by allocation identity and preserve callback boundaries

Hash the payload outside both interner locks. Removal uses pointer equality in the raw-entry predicate, avoiding value-equality callbacks while release is locked. A local Arc keeps the payload alive while removing the map owner.

Release the release mutex before shrinking the shard, preserving the original shard-only rehash environment. This allows a rehash callback to release a nonfinal handle of the same type. Unlock the shard before dropping the retained final Arc, allowing payload destructors to recursively release values from the same shard.

The local Arc is declared before guards, so unwinding releases guards before that owner can destroy the payload.

### Keep a simple per-type lock

A shard lock on every nonfinal drop requires hashing the payload on the hot path, which is undesirable for large strings. Pointer-striped release locks could reduce contention between distinct allocations, but add synchronization machinery. The per-type mutex provides a small, auditable protocol; further optimization should be driven by representative profiling.

## Risks / Trade-offs

- Distinct allocations of one type now contend on a release mutex. Synthetic parallel release workloads show a measurable cost; their timings vary with the host.
- Manual ownership requires the consumed field to stay private and inaccessible after take. The safety comment, manual trait implementations, and representation tests document this constraint.
- The uninstrumented concurrent regression is scheduler-dependent. Repeated bounded runs and independent review supplement it; a separate extracted reproduction also forces the original legal schedule.
- Short editor trials test responses and bounded resource use, not long-term reclamation benefits or statistical performance equivalence.

## Validation and Rollout

The implementation has eight interner-focused tests covering concurrent releases/reinterning, eight callers, same-shard recursion, callbacks, collisions, and representation. All 55 analysis unit tests and repeated bounded core tests passed using Rust 1.99 with the matching v0.15.8 dependency baseline. The affected source is identical in that baseline and the PR base.

Full current-PR workspace formatting passed. Strict local analysis Clippy finds the same existing `unnecessary_sort_by` warnings on baseline and candidate; allowing only that lint passes with other warnings denied. Matching release LSP builds and bounded synthetic protocol trials passed using the same dependencies and a separate pre-existing query-history GC patch, which is outside this change.

Repository Rust 1.92 checks are not yet run: that toolchain is absent from the offline environment. Hosted CI is awaiting upstream approval. OpenSpec CLI validation is also unrun because the CLI is unavailable offline; artifacts follow the checked-in spec-driven schema and were checked structurally.

No migration or configuration change is required. Integrators carrying the local patch can remove it when adopting an upstream revision containing the fix, after verifying the source change and rerunning relevant checks.
