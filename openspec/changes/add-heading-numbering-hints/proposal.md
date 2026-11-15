## Why

Typst resolves heading numbers only when the document is compiled, so authors reading the source see `== Background` and have to open the PDF to learn that it is section 1.1. Tinymist already has the compiled document at hand when answering inlay hint requests and when it builds the preview outline, but it does not surface the numbers anywhere. Issue #2129 collects votes for exactly this.

## What Changes

- Add an inlay hint that shows the resolved numbering of a heading, such as `1.2`, after the `==` marker of a markup heading and after a `#heading(..)` call.
- Prefix titles in the preview outline (the `tinymist/documentOutline` notification) with the same numbering, matching how Typst titles PDF bookmarks.
- Both read the `numbers` field that Typst synthesizes on heading elements during compilation, so no extra evaluation is needed. Headings without numbering, and documents that have not compiled successfully, are unchanged.

## Capabilities

### New Capabilities
- `heading-numbering-hints`: Show the resolved numbering of headings in the editor via inlay hints and in the preview outline.

### Modified Capabilities

None.

## Impact

- `crates/tinymist-query/src/inlay_hint.rs` gains a heading case and a lazily built span-to-numbering map from the compiled document.
- `crates/typst-preview/src/outline.rs` carries the numbering on `HeadingNode` and uses it in the outline title.
- Editors that render the outline (VS Code tree view and the preview frontend) show numbered titles with no changes on their side.
- Out of scope: numbering for figures, equations, and theorem environments provided by packages, and a user setting to turn the hint off.
