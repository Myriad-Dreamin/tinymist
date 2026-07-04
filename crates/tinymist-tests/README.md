# tinymist-tests

Shared test support for Tinymist crates.

This crate keeps the existing snapshot helpers and also provides an aggregate
`mock` module for mock-backed VFS, world, and project-runtime tests.

## Mock Support

Mock implementations live in the crate that owns the behavior being mocked:

- `tinymist_vfs::mock`: in-memory workspaces, path access, file mutation
  helpers, `FileChangeSet`, `FilesystemEvent`, and VFS application helpers.
- `tinymist_world::mock`: deterministic `CompilerUniverse` and
  `CompilerWorld` builders backed by `tinymist_vfs::mock`, embedded Typst
  fonts, and `DummyRegistry`.
- `tinymist_project::mock`: project compiler helpers that drive mock changes
  through `Interrupt::Fs` or `Interrupt::Memory`.
- `tinymist_tests::mock`: aggregate re-exports for crates that already use
  `tinymist-tests` as shared test support.

Keep new mock primitives in the lowest owning crate that can define them. For
example, VFS-only helpers belong in `tinymist-vfs`, not here, so
`tinymist-vfs` can test itself without depending on `tinymist-tests`.

## VFS-Only Tests

Use `tinymist_vfs::mock` directly inside `tinymist-vfs`, or through
`tinymist_tests::mock` from downstream crates:

```rust
use tinymist_vfs::mock::MockWorkspace;

let workspace = MockWorkspace::default_builder()
    .file("main.typ", "#let value = [before]\n#value")
    .build();
let mut vfs = workspace.vfs();
let main = workspace.file_id("main.typ").unwrap();

assert_eq!(vfs.source(main).unwrap().text(), "#let value = [before]\n#value");

workspace
    .update_source("main.typ", "#let value = [after]\n#value")
    .apply_to_vfs(&mut vfs);

assert_eq!(vfs.source(main).unwrap().text(), "#let value = [after]\n#value");
```

Use this layer when the test only needs VFS reads, path resolution, or explicit
`FileChangeSet` / `FilesystemEvent` shapes.

## World-Level Tests

Use `tinymist_world::mock` when the test needs a `CompilerUniverse` or
`CompilerWorld`:

```rust
use tinymist_vfs::mock::MockWorkspace;
use tinymist_world::mock::{MockWorkspaceWorldExt, MockWorldChangeExt};

let workspace = MockWorkspace::default_builder()
    .file("main.typ", "#import \"content.typ\": value\n#value")
    .file("content.typ", "#let value = [before]")
    .build();
let mut universe = workspace.world("main.typ").build_universe().unwrap();

workspace
    .update_source("content.typ", "#let value = [after]")
    .apply_to_universe(&mut universe);

let content_path = workspace.path("content.typ");
assert_eq!(
    universe.snapshot().source_by_path(&content_path).unwrap().text(),
    "#let value = [after]",
);
```

This layer avoids the host filesystem, system-font discovery, and package
network access.

## Project Runtime Tests

Use `tinymist_project::mock` when the test should drive the project compiler
through runtime interrupts:

```rust
use tinymist_project::mock::{MockProjectBuilderExt, MockProjectChangeExt};
use tinymist_vfs::mock::MockWorkspace;
use tinymist_world::mock::MockWorkspaceWorldExt;

let workspace = MockWorkspace::default_builder()
    .file("main.typ", "#let value = [before]\n#value")
    .build();
let (mut compiler, _notify_rx) = workspace
    .world("main.typ")
    .project_compiler::<()>()
    .unwrap();

workspace
    .update_source("main.typ", "#let value = [after]\n#value")
    .apply_as_fs_to_project(&mut compiler, false);

assert!(compiler.primary.reason.by_fs_events);
```

Prefer this layer for invalidation, rename/remove, dependency, or recompilation
regressions that should follow the same event path as the runtime.
