## Context

Inlay hints in `tinymist-query` are computed by walking the syntax tree of one source file. Until now they only used syntactic and signature information. The analysis context also exposes the last successfully compiled document through `LocalContext::success_doc`, which hover and go-to-definition already use for labels and bibliography entries.

The preview outline in `typst-preview` is built from the compiled document's introspector by querying heading elements and nesting them by level. It uses the heading body as the title.

## Goals / Non-Goals

**Goals:**
- Show heading numbers in the source editor without evaluating anything beyond what compilation already produced.
- Keep the hint absent, rather than wrong, whenever the compiled document is missing or stale for a heading.
- Make the preview outline consistent with the PDF outline that Typst exports.

**Non-Goals:**
- Numbering for figures, equations, or package-defined environments such as theorems. Those need realizing show rules on references and are a separate design.
- A configuration option. The existing `InlayHintConfig::smart` table gets a flag so one can be wired later; this change does not add a user-facing setting.
- Hints for headings whose span does not match a syntax node, for example headings produced inside a show rule.

## Decisions

### 1. Read the synthesized `numbers` field instead of evaluating counters

Typst's `HeadingElem` has an internal synthesized field `numbers: Option<EcoString>` that the heading's `prepare` step fills with the plain text of the displayed counter. It exists for PDF bookmarks, which face the same constraint as a language server: no engine or styles at hand. Reading it avoids constructing an `Engine` and keeps the hint identical to what the PDF shows.

Alternative considered: build an `Engine` with the real introspector and call `Counter::display_at`. Rejected because it duplicates work Typst already did and would need the heading's style chain to be correct.

### 2. Match headings to syntax nodes by span

The evaluator spans a markup heading's element with the `Heading` node span, and a `#heading(..)` call's element with the `FuncCall` node span. The worker builds a `HashMap<Span, EcoString>` from the introspector once per request, lazily on the first heading or call it meets, and looks each node up in it. Spans include the file id, so headings in included files resolve correctly against the main document.

### 3. Hint placement

For a markup heading the hint goes at the end of the `HeadingMarker` child with left padding, so `== Title` reads as `== 1.2 Title`. For a function call the hint goes at the end of the call node. Both use no `InlayHintKind`, since neither the parameter nor the type kind describes it.

### 4. Preview outline titles

`HeadingNode` gains `numbers: Option<EcoString>` copied from the element, and `outline_item` formats the title as `"{numbers} {body}"` when present. This matches `typst-pdf`'s outline title construction exactly.

## Risks / Trade-offs

- A stale compiled document can briefly show an old number after an edit until the next compile. This is the same staleness hover already has, and hints refresh on the next inlay hint request.
- Headings produced by show rules or packages have detached or non-matching spans and get no hint. Acceptable for a first iteration.

## Validation

- Snapshot fixtures under `crates/tinymist-query/src/fixtures/inlay_hints/` for a compiled document with numbered and unnumbered headings, including a `#heading(..)` call, and for an uncompiled document that must yield no hints.
- `cargo clippy -p tinymist-query -p tinymist-preview --all-targets` with `RUSTFLAGS=-Dwarnings`.
