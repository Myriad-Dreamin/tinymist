## 1. Specification

- [x] 1.1 Define standalone SVG page handling and raster-resource reuse requirements.
- [x] 1.2 Document renderer compatibility and resource scope.

## 2. SVG Export Integration

- [x] 2.1 Add `reflexo-vec2svg` to `tinymist-task` and forward the existing `no-content-hint` compatibility feature.
- [x] 2.2 Render selected pages with Reflexo while preserving paged output indices.
- [x] 2.3 Preserve merged output dimensions and configured page gaps.

## 3. Tests And Validation

- [x] 3.1 Add task-level tests for paged output dimensions and page indices.
- [x] 3.2 Add task-level tests for page selection and merged output gaps.
- [x] 3.3 Run formatting, tests, Clippy, and build checks for `tinymist-task`.
- [x] 3.4 Add a Tinymist-level repeated-raster export regression test.

## 4. Dependency Integration

- [x] 4.1 Replace local path overrides with compatible published versions or one exact typst.ts revision before proposing the portable Tinymist change.
