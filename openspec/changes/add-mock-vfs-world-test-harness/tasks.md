## 1. Establish shared mock runtime support

- [ ] 1.1 Add shared test-support plumbing that can be reused from multiple workspace crates.
- [ ] 1.2 Implement an in-memory workspace model plus mock path access for Typst source files.
- [ ] 1.3 Add deterministic world/universe builders that use embedded or in-memory fonts and dummy package behavior.

## 2. Drive file-manipulation flows through the runtime

- [ ] 2.1 Add helpers for create, update, rename, and remove operations on the mock workspace.
- [ ] 2.2 Add helpers that expose or apply the corresponding `FileChangeSet` or `FilesystemEvent` flow to VFS/world/project tests.
- [ ] 2.3 Document the intended test layers so callers can choose VFS-only, world-level, or project-level coverage without reimplementing setup.

## 3. Seed initial regression coverage

- [ ] 3.1 Add focused tests that prove the harness can build a runtime from in-memory files and exercise follow-up mutations.
- [ ] 3.2 Add an initial rename/remove regression-shaped test that can serve as the precursor for the `fix-source-rename-cache-invalidation` work.
- [ ] 3.3 Run focused Rust tests for the touched crates and review the results.
