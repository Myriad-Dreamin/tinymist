Authored with Codex (GPT-6).

## 1. Repair Final-Caller Reclamation

- [x] 1.1 Inspect the original count-check/decrement race and reproduce retained map ownership.
- [x] 1.2 Serialize caller count inspection and decrement per interned type.
- [x] 1.3 Recheck removal under shard then release locks and match canonical allocation identity.
- [x] 1.4 Move ownership into a local Arc without losing nested-option niches.
- [x] 1.5 Preserve hash callbacks and unlock guards before final payload destruction.

## 2. Add Regression Coverage

- [x] 2.1 Cover concurrent final drops, reinterning, and eight callers releasing unique payloads.
- [x] 2.2 Cover recursive same-shard destruction and nonfinal releases from shrinking hash callbacks.
- [x] 2.3 Cover pointer-identity removal without equality callbacks.
- [x] 2.4 Assert sized and unsized handle and nested-option representations.

## 3. Validate

- [x] 3.1 Run focused interner and all analysis unit tests on the matching dependency baseline with Rust 1.99.
- [x] 3.2 Run repeated bounded core regressions and obtain independent ownership/unwind review.
- [x] 3.3 Check formatting across the PR workspace.
- [x] 3.4 Compare local Clippy with the original baseline and report the existing lint limitation.
- [x] 3.5 Build matched release LSP binaries and compare bounded synthetic protocol responses and resource use.
- [x] 3.6 Check the OpenSpec artifacts against checked-in schema/examples and validate requirement/scenario structure.
- [ ] 3.7 Run repository-toolchain Rust 1.92 focused tests and strict checks; unavailable in the offline environment.
- [ ] 3.8 Run OpenSpec CLI validation; CLI unavailable in the offline environment.
- [ ] 3.9 Complete maintainer-gated hosted CI without describing unrun checks as passed.
