## Why

The native Tinymist viewer currently renders paged preview content as a Vello canvas without preserving link semantics. Links therefore look like regular content and clicks always go through source-position sync instead of opening supported external destinations.

## What Changes

- Add a semantic layer alongside rendered Vello pages for interactive rectangles.
- Capture `Link` vector items as semantic link rectangles in page coordinates.
- Hit-test semantic links before source-position sync on page clicks.
- Open only external `http`, `https`, and `mailto` links through the system default handler.

## Capabilities

### New Capabilities

- `native-viewer-link-actions`: The native viewer can preserve link hit areas and open supported external links.

### Modified Capabilities

None.

## Impact

- `crates/tinymist-viewer/src/render.rs` records link semantics while rendering vector items.
- `crates/tinymist-viewer/src/lib.rs` exposes page/link semantic data and hit-testing helpers.
- `crates/tinymist-viewer/src/incr.rs` carries semantics with flushed pages.
- `crates/tinymist-viewer/src/main.rs` opens supported external links before falling back to source sync.
- `crates/tinymist-viewer/Cargo.toml` uses the existing workspace `open` dependency.
