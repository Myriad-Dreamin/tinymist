## 1. Retire renamed dependency paths at runtime

- [ ] 1.1 Update the rename/removal invalidation flow so a depended source path clears stale VFS/source-cache state before later compilations can reuse it.
- [ ] 1.2 Change the harmless-VFS compile skip logic to treat paths from the last successful dependency set as recompilation-worthy when those paths are renamed or removed.
- [ ] 1.3 Verify project entry/focus handling and downstream compile consumers stop serving the retired path after a rename.

## 2. Add regression coverage

- [ ] 2.1 Add a focused regression test that compiles a document depending on `content.typ`, renames or removes that file, and proves later compilation no longer serves the old path from cache.
- [ ] 2.2 Add a follow-up update case showing that editing `base.typ` or the renamed file after the rename keeps diagnostics and preview results aligned with current workspace state.

## 3. Validate the fix

- [ ] 3.1 Run focused tests for the touched Rust crates and any new regression fixtures.
- [ ] 3.2 Review the test outputs to confirm rename/removal of depended source files no longer falls through the harmless-VFS path.
