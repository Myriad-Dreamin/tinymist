## Context

`tinymist-viewer` consumes preview `diff-v1` frames, merges them into `IncrDocClient`, converts changed vector IR pages into `VecPage`, and returns a `Vec<(Arc<Scene>, Size)>` to the Xilem UI. `IncrVelloPass::interpret_changes` already reuses unchanged `VecPage` values by comparing page `content_hash`, but `IncrVelloDocClient::render_pages` still calls `flush_page` for every page after every merge.

For large Typst documents, `flush_page` recursively appends the cached vector scene into a fresh `vello::Scene`, so a one-page edit still rebuilds thousands of page scenes. The viewer can avoid that cost because page hashes are already available and the UI accepts `Arc<Scene>` values.

## Goals / Non-Goals

**Goals:**

- Cache flushed Vello scenes per page.
- Reuse cached scene arcs for pages whose content hash and size are unchanged.
- Rebuild only the flushed scenes for changed pages.
- Keep the existing `render_pages` API shape so the UI and websocket code remain unchanged.
- Add unit tests for cache semantics and a benchmark target for tens, hundreds, and thousands of pages.

**Non-Goals:**

- Viewport virtualization or only rendering visible pages.
- GPU damage tracking inside Vello or WGPU.
- Preview protocol changes.
- UI scroll state or source-to-preview synchronization work.

## Decisions

The flushed scene cache lives in `IncrVelloPass` next to the vector page cache. This keeps both cache levels under the same content hash comparison and avoids adding UI-level knowledge of dirty pages.

Each cached flushed page stores the page `content_hash`, page `size`, and `Arc<Scene>`. A cache hit requires all three to match the current `VecPage`, so a page with identical content but changed dimensions still refreshes its returned size and scene. The cache vector is rebuilt to match the current page order and length, which naturally drops removed pages.

`render_pages` continues to return the full page list. Returning only dirty pages would require changing the UI state model and reconciling page insertions/removals. Full-list return with per-page `Arc` reuse is a smaller change and is enough for Xilem widgets and tests to observe unchanged page identity.

`PageCanvas::request_render` skips `ctx.request_render()` when the rebuilt view passes the same scene allocation, scale, and background color. This connects the render cache to the real UI path: Xilem may rebuild every page view from the full page list, but unchanged page canvases do not need to repaint if their inputs are identical.

The benchmark uses generated Typst fixtures and the same preview delta path used by the viewer. It measures the initial full render, a subsequent no-op incremental render, and an incremental render after changing one middle page for 32, 256, and 2048 pages. It also includes a Masonry `TestHarness` that builds a `Portal` containing the vertical page `Flex` and `PageCanvas` widgets, updates the existing page widgets in place, and optionally renders the visible viewport. This covers the page-count scaling risk without depending on an external Typst tests checkout.

## Risks / Trade-offs

- Cached scenes increase memory use by keeping both vector scenes and flushed Vello scenes. This is the intended trade-off for large-document update latency, and reset/page removal releases stale entries.
- The single-page benchmark covers one common edit shape, but it does not measure page insertion/removal or broad layout shifts. Future benchmarks can add those once the viewer supports viewport-level reconciliation.
- The viewer still constructs the full returned page list on each update. This is much cheaper than flushing every page, but future viewport rendering can reduce UI work further.
- The Masonry harness is closer to the viewer than `render_pages`, but it still bypasses the actual Xilem event loop and websocket task scheduling. It is intended to capture widget update, layout, clipping, and paint costs, not operating-system window behavior. The render variants include headless WGPU readback, so update-only variants are included to separate UI update scaling from screenshot rendering overhead.
