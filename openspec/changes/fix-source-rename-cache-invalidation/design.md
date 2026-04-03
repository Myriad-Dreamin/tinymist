## Context

Issue `#2359` reports that renaming a depended Typst source file can leave Tinymist compiling and previewing the old path's cached contents. The logs show a rename from `content.typ` to `new_name.typ`, followed by successful recompiles that still behave as if `content.typ` were present, and later filesystem events being treated as harmless.

The current flow spans several layers:

- `crates/tinymist-query/src/will_rename_files.rs` computes workspace edits for rename-aware clients, but that only affects import text and does not define runtime cache invalidation.
- `crates/tinymist-project/src/compiler.rs` applies filesystem and memory updates into the VFS and records dependency paths for watched projects.
- `crates/tinymist/src/project.rs` decides whether a VFS-only change can skip recompilation by calling `is_clean_compile` against the last compilation's dependent file IDs.
- `crates/tinymist-vfs/src/lib.rs` invalidates cached bytes and parsed source entries when a known path or file ID changes.

That arrangement makes rename/removal bugs subtle: a path can retire even when the next successful path identity is not the same file ID, and the current harmless-VFS optimization reasons primarily over previously compiled file IDs instead of the concrete dependency paths the project last used.

## Goals / Non-Goals

**Goals:**
- Ensure a rename or removal of a depended Typst source path cannot leave stale source contents active for later compilation or preview.
- Make the harmless-VFS optimization treat retired dependency paths as recompilation-worthy changes.
- Reuse existing dependency-path tracking where possible instead of inventing a second rename-specific pipeline.
- Add regression coverage that reproduces the rename sequence from `#2359`, including follow-up edits after the rename.

**Non-Goals:**
- Fixing import-path rewrite behavior for `workspace/willRenameFiles`; that is the separate bug tracked by `#2358`.
- Redesigning the entire VFS cache architecture or file-watching subsystem.
- Changing unrelated preview refresh policy beyond what is needed to stop serving stale renamed-path content.

## Decisions

### 1. Treat retired dependency paths as a first-class invalidation signal

The fix should classify a filesystem rename or removal of a path used by the last successful compilation as a dependency-affecting change, even before considering whether surviving file IDs still look clean. The project compiler already records dependency filesystem paths (`proj.deps`) after each successful compile, so the runtime can use those paths directly when deciding whether a VFS-only change is harmless.

Alternative considered:
- Continue basing the skip decision only on `CompiledArtifact::depended_files()`. Rejected because file IDs describe surviving semantic identities, while `#2359` is specifically about a previously used path disappearing or moving away.

### 2. Invalidate stale source state at the VFS/cache boundary for renamed or removed paths

Once a depended path is retired, Tinymist should explicitly invalidate the old path's cached bytes and parsed source state before later compilation or preview refreshes can reuse it. The existing `invalidate_path` and `invalidate_file_id` flow in `tinymist-vfs` is the right boundary; the implementation should audit that removed or renamed dependency paths always reach that invalidation path before harmless-change filtering can suppress recompilation.

Alternative considered:
- Patch only higher-level preview or project-entry state. Rejected because the stale content originates from lower-level cached source state and would still be reachable through other compile paths.

### 3. Keep rename handling scoped to observed workspace state, not to client rename support

The correctness guarantee should come from observing filesystem state and dependency retirement, not from assuming the client successfully delivered `workspace/willRenameFiles` edits. If path rewrites succeed, recompilation should follow the renamed file. If they do not, recompilation should surface the missing old path instead of serving cached content from it.

Alternative considered:
- Depend on `willRenameFiles` to keep caches and entry state correct. Rejected because `#2359` must still behave correctly when rename edits are empty or when a rename happens outside the editor's assisted path.

### 4. Add regression coverage at the project/runtime layer

The regression should exercise the runtime rename sequence: compile a document that depends on `content.typ`, rename or remove that file, then trigger a follow-up edit or refresh and assert that Tinymist no longer serves `content.typ` from cache. A focused project/VFS test is preferred, with an e2e fixture added only if the lower-level test cannot express the bug clearly.

Alternative considered:
- Rely only on manual validation through preview. Rejected because the failure involves the interaction between VFS invalidation and compile-skip heuristics, which is easy to regress without automated coverage.

## Risks / Trade-offs

- [Path-based invalidation could trigger more recompiles than today] -> Mitigate by only treating paths from the last successful dependency set as non-harmless and leaving unrelated filesystem churn on the existing fast path.
- [The stale result may involve more than one cache layer] -> Mitigate by auditing both the harmless-VFS decision in `crates/tinymist/src/project.rs` and the concrete invalidation path in `crates/tinymist-vfs/src/lib.rs`.
- [A narrowly scoped test might miss preview-specific manifestations] -> Mitigate by asserting compile output or diagnostics freshness in the core regression test and, if needed, adding one preview-facing follow-up check.

## Migration Plan

1. Update the runtime invalidation logic so retired dependency paths force recompilation and retire stale cache state.
2. Add regression coverage for the rename/removal sequence from `#2359`.
3. Validate with focused tests in the touched crates.
4. Rollback, if needed, is a straight revert because the change only affects in-memory invalidation and compile scheduling.

## Open Questions

- Whether the final implementation can satisfy the regression entirely through project/VFS tests or whether one editor-facing/e2e test is needed to capture the exact rename timing seen in VS Code.
