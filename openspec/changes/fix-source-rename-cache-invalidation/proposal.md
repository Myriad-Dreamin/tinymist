## Why

Tinymist can keep compiling and previewing a removed Typst source after that file has been renamed. Issue `#2359` shows that once `content.typ` becomes `new_name.typ`, dependent files can continue reading cached `content.typ` content instead of reflecting the filesystem's current state.

That leaves the language service out of sync with the workspace, hides real missing-file failures behind stale results, and makes rename-heavy workflows unreliable until the server is restarted.

## What Changes

- Invalidate cached Typst source state when a depended source file is renamed or removed so later compilation observes the filesystem's current state.
- Ensure rename and removal events for depended sources are not classified as harmless VFS changes when they retire a path used by the last successful compilation.
- Keep project entry and focused-file state aligned with renamed paths instead of allowing the old path's cached contents to survive as an active source.
- Add regression coverage for source-file rename flows, including follow-up filesystem or editor updates after the rename.

## Capabilities

### New Capabilities
- `source-file-rename-invalidation`: dependent Typst compilation and preview stay consistent with filesystem renames and removals of source files.

### Modified Capabilities

## Impact

- `crates/tinymist/src/project.rs`
- `crates/tinymist-project/src/compiler.rs`
- `crates/tinymist-vfs/src/lib.rs`
- `crates/tinymist-world/src/source.rs` and related world/VFS cache plumbing
- Regression tests covering rename-triggered recompilation and preview freshness
