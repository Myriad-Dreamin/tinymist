## 1. Page Scene Cache

- [x] 1.1 Add a flushed-page cache to `IncrVelloPass` keyed by page content hash and size.
- [x] 1.2 Make `render_pages` reuse cached `Arc<Scene>` values for unchanged pages and refresh changed pages.
- [x] 1.3 Ensure reset clears flushed page scenes while preserving viewer fill configuration.
- [x] 1.4 Skip `PageCanvas` repaint requests when scene, scale, and background are unchanged.

## 2. Cache Semantics Tests

- [x] 2.1 Add unit coverage proving unchanged pages keep flushed scene identity.
- [x] 2.2 Add unit coverage proving changed pages get newly flushed scenes.
- [x] 2.3 Keep existing renderer and preview-frame tests passing.

## 3. Performance Benchmark

- [x] 3.1 Add a repeatable viewer benchmark for 32, 256, and 2048 generated Typst pages.
- [x] 3.2 Report initial full rendering, incremental no-op rendering, and incremental single-page rendering for each page count.
- [x] 3.3 Add a Masonry page-list harness benchmark that includes in-place page widget updates, layout, clipping, visible rendering, and update-only variants.

## 4. Validation

- [x] 4.1 Run focused `tinymist-viewer` tests.
- [x] 4.2 Run the large-page benchmark and record the observed timings.

Validation snapshot from `cargo bench -p tinymist-viewer --bench page_incremental_rendering`:

- Pure `render_pages`, mean time: 32 pages `41.138 us` full / `11.087 us` no-op / `14.012 us` one-page change; 256 pages `316.66 us` / `84.207 us` / `89.142 us`; 2048 pages `2.6987 ms` / `778.83 us` / `800.13 us`.
- Masonry page-list widget update-only, mean time: 32 pages `5.0105 us` no-op / `5.8989 us` visible-page change / `6.3377 us` middle-page change; 256 pages `22.622 us` / `23.431 us` / `23.420 us`; 2048 pages `232.18 us` / `235.21 us` / `241.91 us`.
- Masonry page-list update+render variants were about `55-62 ms` in this headless environment and are dominated by WGPU/TestHarness render readback rather than page update work.
