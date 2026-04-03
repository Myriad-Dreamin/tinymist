## Context

Tinymist already has several pieces that make mocked runtime testing possible in principle:

- `tinymist-vfs` exposes `DummyAccessModel`, `FilesystemEvent`, and direct VFS invalidation primitives.
- `CompilerUniverse::new_raw` can construct worlds from caller-provided VFS, registry, fonts, and entry state.
- `tinymist-project` already records dependency paths and routes filesystem changes through compiler/runtime layers.

What is missing is a reusable way to combine those pieces into deterministic tests. Current coverage mostly falls into two categories: narrow smoke tests inside individual crates and higher-level end-to-end tests under `tests/`. That leaves a gap for bugs like `#2359`, where we want to model a sequence of file mutations and assert cache, dependency, or recompilation behavior without relying on the real filesystem or editor integration.

Because the desired tests span `tinymist-vfs`, `tinymist-world`, and `tinymist-project`, the test infrastructure itself needs to be reusable across crate boundaries.

## Goals / Non-Goals

**Goals:**
- Provide a reusable mock-backed Rust test harness for VFS/world/project runtime behavior.
- Let tests create deterministic workspaces from in-memory file state and mutate that state with file operations such as create, update, rename, and remove.
- Avoid real filesystem, package-network, and system-font dependencies where the tested behavior does not require them.
- Seed the harness with at least one runtime regression path that demonstrates the file-manipulation flows needed by `#2359`.

**Non-Goals:**
- Replacing existing e2e/editor integration tests.
- Simulating OS file watcher quirks in full fidelity.
- Solving the `#2359` cache bug in this change; this change prepares the harness that the fix can rely on.
- General-purpose mocking of unrelated systems such as preview HTTP transport or editor configuration.

## Decisions

### 1. Put reusable test support in a shared workspace crate or equivalent shared support module

The harness should live in shared Rust test support that multiple crates can depend on, rather than being hidden behind `#[cfg(test)]` inside one crate. This avoids dependency inversion problems and lets VFS, world, project, and later server-side tests all reuse the same workspace builder and mutation helpers.

Alternative considered:
- Add ad hoc helpers separately inside `tinymist-vfs`, `tinymist-world`, and `tinymist-project`. Rejected because the bug class we care about crosses those boundaries and duplicated helpers would drift quickly.
- Place everything under the existing `tests/` e2e crate. Rejected because lower-level crate tests should be able to use the harness directly without going through e2e-only structure.

### 2. Model the filesystem as an in-memory workspace with explicit mutations

The harness should keep an in-memory map of workspace paths to file snapshots and expose mutation helpers such as `write`, `remove`, and `rename`. Those helpers should be able to return or apply the corresponding `FileChangeSet`/`FilesystemEvent` shape so tests can drive the same invalidation path the runtime uses.

Alternative considered:
- Use temp directories and real filesystem mutations for all tests. Rejected because those tests are slower, harder to make deterministic, and less suitable for precisely driving intermediate runtime states.

### 3. Build test worlds from deterministic runtime components

World/universe construction in the harness should use embedded or in-memory fonts plus dummy package behavior so tests do not rely on host-specific system state. The harness should wrap the mock workspace in a path access model that feeds `Vfs`/`CompilerUniverse::new_raw` consistently across tests.

Alternative considered:
- Reuse the full system-world builder and let tests touch the host filesystem. Rejected because it would make regression coverage for file invalidation sensitive to machine setup and package/network state.

### 4. Start with runtime regression coverage close to the consuming crates

The harness should be introduced together with a small set of focused tests that prove it can express file-manipulation sequences. Those tests should live near the runtime crates that consume the harness, with `#2359`-style rename/remove coverage as the first motivating case.

Alternative considered:
- Land the harness with no real consumer tests yet. Rejected because it would leave the design unproven and make later bug-fix work rediscover missing affordances.

## Risks / Trade-offs

- [A shared support crate adds workspace surface area] -> Mitigate by keeping the public API narrow and clearly test-focused.
- [Mocked runtime tests may diverge from production watcher behavior] -> Mitigate by driving the same `FileChangeSet` and runtime invalidation entry points used in production, while keeping e2e tests for integration coverage.
- [Deterministic fonts/package setup may still be more than some tests need] -> Mitigate by exposing small builders so tests can choose VFS-only, world-level, or project-level setup as needed.

## Migration Plan

1. Add the shared mock runtime test support and wire it into workspace dev-dependencies.
2. Add initial VFS/world/project tests that use the harness for file-manipulation flows.
3. Use the new harness in the follow-up `fix-source-rename-cache-invalidation` implementation work.
4. Rollback, if needed, is a straightforward revert because the change only affects test infrastructure.

## Open Questions

- Whether the shared support should be a dedicated `crates/` member or a lighter-weight shared module arrangement, as long as multiple crate tests can depend on it without duplicating code.
