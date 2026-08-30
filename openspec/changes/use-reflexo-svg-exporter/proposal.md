## Why

Tinymist's CLI SVG export currently uses Typst's SVG exporter while its preview pipeline uses Reflexo's vector representation and SVG renderer. This split prevents CLI exports from benefiting from Reflexo improvements such as emitting repeated raster image data once per standalone SVG document. Image-heavy documents can therefore produce unnecessarily large SVG files even though the preview renderer has the required reusable-resource architecture.

## What Changes

- Render `tinymist compile --format svg` output through `reflexo-vec2svg`.
- Preserve existing paged output, page selection, merged output, and configured page-gap behavior.
- Keep image resources local to each emitted SVG so independently exported pages remain standalone.
- Add focused task-level tests for paged, selected, and merged SVG output.

## Capabilities

### New Capabilities

- `svg-export`: Define Tinymist's standalone SVG export behavior, including page handling and reusable embedded raster resources.

### Modified Capabilities

None.

## Impact

- `crates/tinymist-task` gains a direct dependency on `reflexo-vec2svg` and uses it for `ExportSvgTask`.
- SVG markup structure changes from Typst's exporter output to Reflexo output while remaining standalone SVG.
- Repeated raster-heavy exports become substantially smaller; exact SVG markup is not preserved.
