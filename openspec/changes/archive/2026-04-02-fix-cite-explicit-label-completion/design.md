## Context

`analyze_labels` currently returns bibliography keys alongside document labels, and citation completion narrows that list by using the bibliography split point. Once the completion loop reaches a bibliography entry, it still reuses the generic label insertion text `"<" + label + ">"` for every completion item, including the title-backed completion variant that points at the same key.

That shared insertion path works for bibliography keys that can be written directly as Typst label literals, such as `Russell:1908`, but it fails for keys such as `DBLP:books/lib/Knuth86a` that require `label("...")` inside `#cite(...)`. The issue is localized to completion rendering rather than bibliography discovery or citation resolution.

## Goals / Non-Goals

**Goals:**
- Make citation completion insert syntactically valid Typst for every bibliography key.
- Preserve the shorter `<key>` insertion form for bibliography keys that already work as label literals.
- Keep bibliography title completions and raw-key completions in sync so both variants insert the same citation argument text.
- Add regression fixtures that cover compatible keys, incompatible keys, and prefix completion for the explicit-label case.

**Non-Goals:**
- Changing how Typst resolves bibliography keys at runtime.
- Changing non-citation label completion flows such as `@ref` and general label completion outside `#cite(...)`.
- Reworking bibliography discovery, hover rendering, or other citation-related editor features.

## Decisions

### 1. Keep citation insertion formatting in the completion layer

The completion path should decide how to render a bibliography key into source text when `only_citation` is active. `analyze_labels` already provides the resolved key string and bibliography/document split; adding editor-oriented insertion text to `DynLabel` would couple shared analysis data to one LSP presentation concern.

Alternative considered:
- Store preformatted citation insertion text in `tinymist-analysis::DynLabel`. Rejected because only completion consumes this information today, and the formatting depends on cursor context such as whether the user already typed `<` or `>`.

### 2. Choose between `<key>` and `label("key")` using label-literal validity, not issue-specific heuristics

Citation completion should first determine whether a bibliography key can be represented as a Typst label literal. If it can, tinymist should keep inserting the current `<key>` form. If it cannot, tinymist should fall back to `label("key")`, which is valid for keys such as DBLP entries that include `/`.

Alternative considered:
- Detect only known-problem characters such as `/`. Rejected because it would bake issue-specific heuristics into completion and miss future invalid key shapes.

Alternative considered:
- Always insert `label("key")` for bibliography completions. Rejected because it would unnecessarily regress the concise insertion form that already works for many existing fixtures and user workflows.

### 3. Compute the citation insertion text once per bibliography entry and reuse it for all completion variants

The raw-key completion item and the title-backed completion item for the same bibliography entry should share one computed insertion text. This keeps acceptance behavior consistent regardless of which visible label the user selects.

Alternative considered:
- Let each completion variant build its own insertion text. Rejected because the two branches could drift and reintroduce mismatched behavior for the same bibliography key.

### 4. Lock the behavior with focused completion fixtures

The regression suite should add a bibliography fixture whose key requires explicit `label("...")` syntax and cover both direct-key and title-backed completion entries. Existing compatible-key fixtures such as `Russell:1908` should continue to assert the shorter `<key>` insertion where it remains valid.

## Risks / Trade-offs

- [The compatibility predicate could diverge from Typst label parsing rules] -> Mitigate by basing it on the same label-literal syntax Typst accepts and covering both positive and negative fixture examples.
- [Fallback insertion could mishandle partially typed citation arguments] -> Mitigate by adding a prefix-completion fixture that snapshots the resulting replacement text for an incompatible bibliography key.
- [A narrow citation-only fix could accidentally affect general label completion] -> Mitigate by keeping the new formatting branch gated behind the existing bibliography-only citation path.

## Migration Plan

1. Add a citation-only helper that computes the correct insertion text for a bibliography key.
2. Wire both bibliography completion variants through that helper while leaving non-citation label completion unchanged.
3. Add new completion fixtures and snapshots for explicit-label bibliography keys, and re-run the existing compatible-key citation snapshots to confirm they keep their current insertion form.

## Open Questions

- If Typst does not expose a reusable helper for label-literal compatibility, the implementation will need a small local predicate that matches the accepted label-literal grammar. The fixtures in this change should keep that predicate honest.
