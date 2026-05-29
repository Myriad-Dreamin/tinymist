## Why

Tinymist's runtime bugs around VFS invalidation, world snapshots, and file manipulation are hard to test precisely today. We have smoke tests and end-to-end coverage, but not a reusable mock-backed harness for exercising rename, remove, and follow-up file updates entirely inside Rust.

That gap makes issues like `#2359` slower and riskier to fix because the first step is often inventing test scaffolding instead of expressing the bug directly. A shared harness would let us write deterministic regression tests for VFS/world behavior before landing cache and invalidation fixes.

## What Changes

- Introduce a reusable mock-based Rust test harness for Tinymist VFS, world, and project-runtime tests.
- Provide helpers to build deterministic worlds or universes from an in-memory workspace, using embedded fonts and dummy package behavior instead of real filesystem or network setup.
- Provide file-manipulation helpers for create, update, rename, remove, and corresponding filesystem-style notifications so tests can model runtime invalidation flows.
- Seed the harness with initial regression-oriented coverage for file-manipulation behavior, including the scenario needed to unblock the `#2359` cache-invalidation fix.

## Capabilities

### New Capabilities
- `mock-vfs-world-testing`: reusable mock-backed test support for exercising VFS, world, and file-manipulation behavior in Rust tests.

### Modified Capabilities

## Impact

- A shared Rust test-support module or crate under `crates/` that can be reused across VFS, world, project, and server-side tests
- `crates/tinymist-vfs/`
- `crates/tinymist-world/`
- `crates/tinymist-project/`
- Workspace Cargo manifests and dev-dependencies for the shared harness
- Initial regression tests covering mocked file-manipulation flows
