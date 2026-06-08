## 1. OpenSpec

- [x] 1.1 Add proposal, design, tasks, and tinymist-viewer-zoom spec delta.

## 2. Viewer Zoom

- [x] 2.1 Add bounded zoom scale helpers and tests.
- [x] 2.2 Track zoom state in `PreviewState` and apply it on top of the fit-to-width page scale.
- [x] 2.3 Add page-level keyboard zoom actions while preserving existing source-click behavior.
- [x] 2.4 Replace fixed zoom multiplication with the preview/pdf.js-style zoom factor ladder.
- [x] 2.5 Accumulate modified-wheel delta before emitting zoom actions.
- [x] 2.6 Move modified-wheel zoom handling from page widgets to the viewer scroll portal.
- [x] 2.7 Preserve the cursor anchor by adjusting the portal viewport after wheel zoom layout.
- [x] 2.8 Restore visible, synchronized scrollbars in the viewer scroll portal.
- [x] 2.9 Match modified-wheel zoom direction to native viewer expectations.
- [x] 2.10 Center the page list horizontally when zoomed content is narrower than the viewport.
- [x] 2.11 Simplify `zoom_portal` by consolidating overlay scrollbar geometry and viewport state plumbing.

## 3. Validation

- [x] 3.1 Run targeted `tinymist-viewer` tests.
- [x] 3.2 Run formatting check for touched Rust code.
