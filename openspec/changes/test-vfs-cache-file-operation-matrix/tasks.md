## 1. Define the VFS/cache matrix

- [ ] 1.1 Map `O01..O20` from the Typst decomposition document to VFS/cache expectations.
- [ ] 1.2 Group rows only where VFS/cache postconditions are identical and document the grouping.
- [ ] 1.3 Define relation variants for entry, active dependency, missing dependency, retained inactive dependency, asset dependency, shadow-open path, and unrelated path.

## 2. Add deterministic cache fixtures

- [ ] 2.1 Add mock workspace fixtures for create, content update, transient empty, read error, remove, recreate, atomic replace, file rename, directory rename, root-boundary move, dependency membership changes, shadow filesystem race, symlink-like observable changes, and mixed batches.
- [ ] 2.2 Add helpers to drive normalized `FileChangeSet` shapes through VFS/world/project-runtime state without raw watcher input.
- [ ] 2.3 Add test-only inspection helpers for source lookup, path retirement, read-error snapshots, and compile-cache freshness where existing public test surfaces are insufficient.

## 3. Assert VFS and world state

- [ ] 3.1 Test insert, update, recreate, and atomic replacement rows refresh source bytes and parsed source state.
- [ ] 3.2 Test remove, rename, directory-prefix, and root-boundary rows retire old paths.
- [ ] 3.3 Test read-error and recovery rows replace stale readable snapshots.
- [ ] 3.4 Test retained inactive dependency and unrelated churn variants do not emit stale active state.

## 4. Assert compile-cache freshness

- [ ] 4.1 Test affected entry and dependency rows mark or refresh compile cache before later compile results are reused.
- [ ] 4.2 Test stale-reference rename rows report old paths unavailable instead of using cached old source.
- [ ] 4.3 Test updated-reference rename rows follow new dependency paths and drop old dependency paths.
- [ ] 4.4 Test mixed batches are order-insensitive with respect to final compile-visible workspace state.

## 5. Validate

- [ ] 5.1 Run focused `tinymist-vfs` and `tinymist-world` tests that cover VFS/cache state.
- [ ] 5.2 Run focused `tinymist-project` tests that cover compile-cache freshness.
- [ ] 5.3 Run `cargo fmt --check --all`.
