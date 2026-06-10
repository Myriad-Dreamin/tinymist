## 1. OpenSpec

- [x] 1.1 Add proposal, design, tasks, and native-viewer-title-bar spec.

## 2. Viewer Title Bar

- [x] 2.1 Run `tinymist-viewer` in a decorationless native window.
- [x] 2.2 Add a viewer-owned title bar with help, minimize, maximize, and close controls.
- [x] 2.3 Add a Typst-rendered help overlay for preview interactions.
- [x] 2.4 Pass document titles from the GPU viewer provider and keep side-by-side window matching working.

## 3. Native Window Interaction

- [x] 3.1 Add Windows native title-bar hit testing for drag, resize, and maximize.
- [x] 3.2 Bridge maximize-button non-client mouse events back to the custom title-bar visuals.
- [x] 3.3 Keep non-Windows native title-bar integration as a no-op.

## 4. Validation

- [x] 4.1 Add targeted tests for title-bar geometry, icon paths, help rendering, and native hit testing.
- [x] 4.2 Run targeted `tinymist-viewer` tests.
- [x] 4.3 Run formatting and diff checks.
