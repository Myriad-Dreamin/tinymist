## Context

`RenameRequest` already delegates to `find_references`, so tinymist's rename behavior is only as complete as the reference search it builds on. Today `find_references` ultimately relies on `ExprInfo.get_refs`, which returns lexical and already-resolved semantic references from `resolves`.

Named argument labels are recognized elsewhere in the stack, but not yet surfaced through that reference path. `syntax/expr.rs` records named call arguments as identifier references, and `analysis/call.rs` can map `ast::Arg::Named` nodes to the concrete parameter name for both direct calls and `.with(...)` flows. That mapping is currently used for call-aware editor features such as inlay hints, while references and rename still miss those label sites.

The result is the exact failure reported in `#2444`: renaming a user-defined parameter updates the declaration and in-function uses, but leaves call-site labels unchanged even when tinymist can already understand which parameter they bind to.

## Goals / Non-Goals

**Goals:**
- Include semantically bound named argument labels in parameter references and rename results for user-defined functions and closures.
- Cover both direct calls and `.with(...)`-style partial application without relying on text-only identifier matching.
- Preserve precise edit ranges so rename changes only the label token and not the colon or argument value.
- Add regression coverage that proves unrelated same-name parameters are not renamed.

**Non-Goals:**
- Implementing dictionary-field references or rename support.
- Changing rename support for native/builtin parameters that remain non-renamable today.
- Broadening field-access rename, import rename, or other unrelated roadmap items.

## Decisions

### 1. Extend shared reference discovery instead of special-casing rename

The missing behavior belongs in shared reference lookup, not only in `rename.rs`. `textDocument/rename` should continue to inherit its edit set from `find_references`, while `textDocument/references` gains the same named-argument coverage automatically.

Alternative considered:
- Add rename-only logic in `RenameRequest`. Rejected because it would leave `textDocument/references` inconsistent with rename and duplicate the same semantic matching rules.

### 2. Use semantic call analysis to identify parameter-bound named arguments

Named-argument matches should be discovered from call analysis that already resolves a call's effective signature and named-parameter bindings. The matching key should combine the callable that is being invoked with the bound parameter name so the search only includes labels that actually bind to the renamed parameter, including `.with(...)` signatures derived from the same user-defined function.

Alternative considered:
- Scan for matching identifier text such as every `side:` label in the workspace. Rejected because it would rename unrelated same-name parameters and could not distinguish different callables.

Alternative considered:
- Store the relationship directly in the low-level `ExprInfo.resolves` map during expression building. Rejected because call-signature binding lives later in analysis and forcing it into the earlier syntax stage would increase coupling.

### 3. Compute rename ranges from the label token only

When a named argument is included in a rename result, the returned range should cover only the identifier token of the label. This keeps `side: left` editable as `sd: left` without replacing the colon, whitespace, or value expression.

Alternative considered:
- Reuse the whole `ast::Arg::Named` node range. Rejected because rename would overwrite the entire argument text instead of the semantic label.

### 4. Lock behavior with fixture-based regression tests

The change should add focused reference and rename fixtures for:
- a direct named call on a user-defined function
- a `.with(...)` named argument bound to the same parameter
- an unrelated callable with the same parameter name that must stay unchanged

This keeps the change narrow and makes the expected LSP edits easy to review in snapshots.

## Risks / Trade-offs

- [Call-site matching could include unrelated same-name parameters] -> Mitigate by matching on semantic callable/parameter binding rather than raw identifier text.
- [Reference lookup may do more work for parameter queries] -> Mitigate by restricting the extra search to parameter targets and reusing existing call/signature analysis instead of inventing a second resolver.
- [`.with(...)` chains may differ from direct calls in subtle ways] -> Mitigate by leaning on the same call-analysis path already used for call-aware editor features and covering it with dedicated fixtures.

## Migration Plan

1. Extend shared parameter reference collection to surface named-argument label spans from semantically resolved call sites.
2. Keep `textDocument/references` and `textDocument/rename` on that shared path so the new behavior is consistent.
3. Add focused fixtures and snapshot tests for direct named calls, `.with(...)`, and non-matching same-name parameters.

## Open Questions

- The same general shape may help future dictionary-field reference work, but that broader reuse is intentionally left out of this change.
