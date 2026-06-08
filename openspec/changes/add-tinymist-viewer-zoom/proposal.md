## Why

The native `tinymist-viewer` always fits pages to the window width, which leaves users without a way to inspect details or return to the fitted overview while using the GPU viewer as their primary preview. Zoom controls should be local to the native viewer and should not require changes to Tinymist's preview data protocol.

## What Changes

- Add interactive zoom controls to `tinymist-viewer`.
- Preserve the current fit-to-window behavior as the default and as the reset target.
- Apply zoom as a multiplier on top of the existing page fit scale so document geometry and source-click mapping stay stable.
- Keep the preview server protocol and provider extension contract unchanged.

## Capabilities

### New Capabilities

- `tinymist-viewer-zoom`: Let native viewer users zoom rendered preview pages in, zoom out, and reset to fit-to-window scale.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist-viewer/src/main.rs` will track viewer zoom state and apply it to page sizing.
- `crates/tinymist-viewer/src/doc.rs` will surface keyboard zoom input actions alongside existing click actions.
- `crates/tinymist-viewer/src/zoom_portal.rs` will own modified-wheel zoom across the scrollable viewer area and keep the cursor anchor stable.
- Viewer tests should cover zoom clamping and fit-scale composition where feasible.
