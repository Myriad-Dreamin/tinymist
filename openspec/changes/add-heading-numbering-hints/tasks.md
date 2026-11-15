## 1. Inlay Hints

- [x] 1.1 Add `on_heading_numbering` to `InlayHintConfig` and enable it in `smart()`.
- [x] 1.2 Build a span-to-numbering map from the compiled document's heading elements, lazily per request.
- [x] 1.3 Emit a hint after the marker of markup headings and after `#heading(..)` calls whose span is in the map.
- [x] 1.4 Add snapshot fixtures for compiled numbered headings and for an uncompiled document.

## 2. Preview Outline

- [x] 2.1 Carry the synthesized `numbers` on `HeadingNode`.
- [x] 2.2 Prefix outline titles with the numbering, matching the PDF outline.

## 3. Docs And Validation

- [x] 3.1 Mention heading numbering under inlay hints in `docs/tinymist/feature/language-content.typ` and regenerate derived docs.
- [x] 3.2 Run clippy with warnings denied and the `tinymist-query` test suite.
