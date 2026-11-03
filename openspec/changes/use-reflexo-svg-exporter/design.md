## Context

`ExportSvgTask` previously called `typst-svg` directly. Reflexo now supports document-local raster resource deduplication, so using it for the CLI export avoids repeating image payloads. The task must continue to support page selection, separate page files, and merged output with a gap.

## Goals / Non-Goals

**Goals:**
- Use Reflexo for Tinymist CLI SVG exports.
- Preserve physical page indices in paged output.
- Preserve page selection and vertical merge-gap behavior.
- Keep every emitted SVG self-contained.
- Deduplicate raster resources only within each complete emitted SVG document.

**Non-Goals:**
- Share SVG definitions between separately exported page files.
- Change the preview websocket protocol or incremental document lifetime.
- Guarantee byte-for-byte compatibility with `typst-svg` output.
- Replace embedded image data with external URLs.

## Decisions

### 1. Convert the document once

`ExportSvgTask` converts the complete Typst document with `SvgExporter::svg_doc` and prepares its glyphs once. Page selection then indexes the converted pages by the physical page indices returned from the existing `select_pages` helper.

This avoids converting the document separately for every page and preserves the physical page indices used by output-path templates.

### 2. Keep paged output independent

Without merge configuration, each selected vector page is passed separately to `render_flat_svg`. Each resulting file owns its namespaces and definitions, so it has no dependency on another page's SVG.

Raster deduplication is consequently per page for paged output. Cross-file references would make the files dependent on one another.

### 3. Apply merge gaps to page bounds

For merged output, selected vector pages are cloned and the configured gap is added to every page height except the last. Reflexo stacks pages using those bounds, preserving the existing gap behavior without adding space after the final page.

Invalid gap expressions retain the existing fallback to zero rather than introducing a new error path.

## Risks / Trade-offs

- [SVG structural differences] -> Validate dimensions, page selection, merge gaps, standalone parsing, and representative image-heavy output rather than exact snapshots of exporter-specific markup.
- [Per-page duplicate payloads] -> Accept one definition per independently usable page; merged output deduplicates across all selected pages in its single SVG document.
