## Why

The native GPU viewer already reuses vector page trees when a page content hash is unchanged, but it still flushes every page into a new Vello scene after each preview delta. Large documents therefore pay an O(page count) scene rebuild cost even when only one page changes.

This change makes page-level incremental rendering visible at the Vello scene boundary and adds a repeatable performance harness for documents with tens, hundreds, and thousands of pages.

## What Changes

- Cache flushed Vello scenes per page alongside the page size and content hash.
- Re-render only pages whose content hash or size changes, while reusing the previous `Arc<Scene>` for unchanged pages.
- Avoid requesting a page canvas repaint when the rebuilt viewer view receives the same scene, scale, and background color.
- Clear the flushed scene cache on viewer reset without losing viewer background configuration.
- Add focused tests that prove unchanged pages reuse their flushed scenes and changed pages are refreshed.
- Add a viewer benchmark target that measures initial rendering, no-op incremental rendering, one-page-dirty incremental rendering, and a Masonry page-list paint harness for tens, hundreds, and thousands of generated Typst pages.

## Capabilities

### New Capabilities
- `gpu-viewer-page-incremental-rendering`: Native GPU viewer page rendering cache behavior and performance measurement for large documents.

### Modified Capabilities

## Impact

- Affected code: `crates/tinymist-viewer/src/incr.rs`.
- Affected tests and benchmarks: `crates/tinymist-viewer` unit tests and a new benchmark target under the viewer crate.
- No preview websocket protocol changes.
- No user-facing configuration changes.
