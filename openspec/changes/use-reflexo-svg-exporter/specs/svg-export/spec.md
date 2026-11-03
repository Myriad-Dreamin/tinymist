## ADDED Requirements

### Requirement: Tinymist exports standalone SVG documents through Reflexo

Tinymist SHALL render `ExportSvgTask` output through `reflexo-vec2svg`. Every output SHALL contain the namespaces, definitions, and embedded resources required to open it independently.

#### Scenario: A page is exported independently

- **WHEN** SVG export produces separate page files
- **THEN** each selected page is emitted as a complete standalone SVG document
- **AND** the paged result retains that page's zero-based physical index

### Requirement: SVG export preserves page selection

Tinymist SHALL apply the configured physical page ranges before rendering paged or merged SVG output.

#### Scenario: A subset of pages is selected

- **WHEN** an SVG task selects one or more physical page ranges
- **THEN** only those pages appear in the output
- **AND** separate page results retain their original physical page indices

### Requirement: SVG export preserves merged layout

Tinymist SHALL stack selected pages vertically for merged output and SHALL insert the configured gap between adjacent pages without adding a trailing gap.

#### Scenario: Selected pages are merged with a gap

- **WHEN** an SVG task requests merged output with a valid gap
- **THEN** the merged document height equals the sum of selected page heights plus one gap for each adjacent page pair
- **AND** the merged width accommodates the widest selected page

#### Scenario: The configured gap is invalid

- **WHEN** an SVG task requests merged output with a gap expression that cannot be parsed as a length
- **THEN** Tinymist uses a zero-length gap

### Requirement: Repeated raster resources are embedded once per SVG document

Tinymist SHALL rely on the Reflexo exporter to emit one embedded definition for each unique PNG, JPEG, GIF, or WebP resource used in a complete SVG document. Each placement SHALL reference that definition while retaining its placement-specific rendering properties.

#### Scenario: One raster image is placed repeatedly

- **WHEN** the same raster resource appears multiple times in one emitted SVG document
- **THEN** the SVG contains one embedded payload for that resource
- **AND** it contains one reference for every placement

#### Scenario: Distinct raster images are placed

- **WHEN** raster resources have different formats or embedded byte content
- **THEN** the SVG contains distinct definitions for those resources

#### Scenario: Pages are exported separately

- **WHEN** the same raster resource appears on multiple independently exported pages
- **THEN** each page SVG may contain its own definition
- **AND** no page references a definition owned by another SVG document
