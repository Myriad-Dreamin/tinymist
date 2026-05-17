## 1. Establish shared mock runtime support

- [x] 1.1 Add shared test-support plumbing that can be reused from multiple workspace crates.
- [x] 1.2 Implement an in-memory workspace model plus mock path access for Typst source files.
- [x] 1.3 Add deterministic world/universe builders that use embedded or in-memory fonts and dummy package behavior.

## 2. Drive file-manipulation flows through the runtime

- [x] 2.1 Add helpers for create, update, rename, and remove operations on the mock workspace.
- [x] 2.2 Add helpers that expose or apply the corresponding `FileChangeSet` or `FilesystemEvent` flow to VFS/world/project tests.
- [x] 2.3 Document the intended test layers so callers can choose VFS-only, world-level, or project-level coverage without reimplementing setup.

## 3. Seed initial regression coverage

- [x] 3.1 Add focused tests that prove the harness can build a runtime from in-memory files and exercise follow-up mutations.
- [x] 3.2 Add an initial rename/remove regression-shaped test that can serve as the precursor for the `fix-source-rename-cache-invalidation` work.
- [x] 3.3 Run focused Rust tests for the touched crates and review the results.

## 4. Keep mocks usable inside their owning crates

- [x] 4.1 Move VFS mock workspace/access/change helpers into `tinymist-vfs`.
- [x] 4.2 Move world mock universe/world builders into `tinymist-world`.
- [x] 4.3 Move project compiler mock event helpers into `tinymist-project`.
- [x] 4.4 Keep `tinymist-tests` as an aggregate re-export and document mock usage in its README.
- [x] 4.5 Add simple `tinymist-vfs` and `tinymist-world` tests that use their crate-local mocks.
- [x] 4.6 Run focused Rust tests and formatting for the touched crates.
