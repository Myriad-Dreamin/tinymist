## Context

`tinymist-viewer` renders preview pages in a native Xilem/Masonry window. `PreviewState::view` currently computes a fit-to-width scale from the window width and each page's scene size, then passes that scale to `PageCanvas`. `PageCanvas` owns page input for source-position clicks and draws the scene with the supplied scale.

The preview data-plane sends document updates only; it should not need a new message to support viewer-local zoom.

## Goals / Non-Goals

**Goals:**

- Add local zoom controls for the native viewer.
- Keep fit-to-width as the startup behavior and reset target.
- Keep source-click coordinate mapping based on document-space page geometry.
- Avoid changing the preview server protocol or VS Code previewer provider API.

**Non-Goals:**

- Persist zoom between viewer launches.
- Add toolbar UI or settings for zoom increments.
- Add pinch gesture support beyond the events currently exposed through Masonry.
- Synchronize zoom state with the webview preview frontend.

## Decisions

1. Store zoom as a `PreviewState` multiplier.

   The viewer already recomputes a per-page fit scale during view construction. A single `zoom_scale` multiplier composes naturally with that fit scale and keeps rendered page sizes consistent across all pages. Resetting zoom to `1.0` restores current behavior.

2. Emit modified-wheel zoom actions from the scroll portal.

   Modified-wheel zoom must work across the full viewer surface, including gaps and areas outside an individual page. The native `zoom_portal` owns wheel events at the scroll viewport layer, accumulates deltas using the same threshold as the web preview, and marks modified-wheel events handled so they do not also scroll the page list. `PageCanvas` still owns page click mapping and keyboard zoom shortcuts when a page has focus.

   The portal also owns overlay scrollbar geometry, painting, and drag handling so replacing Xilem's stock `portal` does not remove the visible scrollbars. Scrollbar movement, ordinary wheel scrolling, keyboard scrolling, accessibility scrolling, and zoom anchor compensation all update the same viewport state.

3. Use familiar modified shortcuts.

   The initial control surface will support `Ctrl`/`Meta` plus mouse wheel for continuous zoom, `Ctrl`/`Meta` plus `+`/`=` for zoom in, `Ctrl`/`Meta` plus `-` for zoom out, and `Ctrl`/`Meta` plus `0` for reset. `Meta` covers common macOS shortcut expectations while `Ctrl` covers Windows/Linux.

4. Use the preview zoom ladder.

   The web preview does not multiply by a constant for every zoom command. It steps through a pdf.js-style factor ladder from `0.1` to `10.0`, with denser stops around the default fitted scale. The native viewer will use the same ladder for keyboard and wheel zoom so repeated commands feel predictable and can land exactly back on `1.0`.

5. Accumulate modified wheel delta before zooming.

   The web preview treats `Ctrl` wheel as zoom input, converts line deltas to pixels at `20px` per line, accumulates wheel distance, and only changes zoom after the absolute distance reaches `20px`. The native viewer will mirror that model for `Ctrl`/`Meta` wheel events. Modified wheel events below the threshold are still handled so they do not also scroll the portal.

6. Preserve the cursor anchor after wheel zoom.

   The web preview compensates the scroll offset after changing `currentScaleRatio` so the document point under the cursor stays under the cursor. The native `zoom_portal` records the viewport position, cursor-local position, and old zoom before submitting a wheel zoom action. After the child layout reflects the new zoom scale, it sets the viewport to `(old_viewport + cursor_local) * (new_zoom / old_zoom) - cursor_local`, clamped to the available scroll range.

7. Center zoomed-out content.

   When the page list is narrower than the scroll viewport, the native `zoom_portal` places the child at `(viewport_width - content_width) / 2` horizontally. Anchor math converts cursor positions through that child origin so centered zoomed-out pages still zoom around the pointer instead of around the viewport origin. When content overflows horizontally, the child origin returns to zero and horizontal scrolling behaves normally.

8. Match native viewer wheel direction expectations.

   The native viewer treats positive wheel delta while holding the zoom modifier as zoom-in and negative wheel delta as zoom-out. This differs from the older web preview model, but matches the direction expected for this native viewer.

## Risks / Trade-offs

- [Scroll conflict] Modified wheel events could otherwise also scroll the portal -> Mark zoom wheel events handled at the portal layer after accumulating the wheel delta.
- [Keyboard focus requirement] Keyboard shortcuts only work after the viewer page is focused -> Request focus on pointer-down, matching existing click behavior.
- [Layout jump on zoom] Rebuilding page dimensions changes portal scroll extents -> Use the viewer-specific `zoom_portal` so wheel zoom can adjust the viewport after the new page layout is known.
- [Scrollbar parity] Masonry's built-in `Portal` owns scrollbar internals that are not exposed across crates -> The viewer-specific portal paints and handles overlay scrollbars from local geometry and keeps their progress synchronized with viewport changes.
- [Shortcut variance] Some keyboard layouts may report `+` differently -> Accept both `+` and `=` for zoom in and rely on modified wheel as a layout-independent path.
