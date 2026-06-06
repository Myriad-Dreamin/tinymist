## ADDED Requirements

### Requirement: Viewer reuses flushed page scenes for unchanged pages
The native GPU viewer SHALL reuse an existing flushed Vello scene for a page when the page content hash and page size are unchanged across preview deltas.

#### Scenario: Unchanged page keeps scene identity
- **WHEN** the viewer renders a document update after a previous render
- **AND** a page has the same content hash and size as the prior render
- **THEN** the returned page entry uses the same flushed scene allocation as the prior render

#### Scenario: Changed page refreshes scene identity
- **WHEN** the viewer renders a document update after a previous render
- **AND** a page has a different content hash or size than the prior render
- **THEN** the returned page entry uses a newly flushed scene for that page

### Requirement: Viewer reset clears page render caches
The native GPU viewer SHALL clear vector and flushed Vello page caches when a reset-style preview frame is processed.

#### Scenario: Reset removes cached page scenes
- **WHEN** the viewer reset path is invoked before merging a full-current preview frame
- **THEN** no flushed page scenes from the previous document remain cached
- **AND** viewer background fill configuration remains unchanged

### Requirement: Viewer skips unchanged page canvas repaint
The native GPU viewer SHALL avoid requesting a page canvas repaint when a rebuilt page view receives the same flushed scene allocation, scale, and background color.

#### Scenario: Rebuilt unchanged page avoids repaint request
- **WHEN** a page view is rebuilt after a preview update
- **AND** the page scene allocation, scale, and background color are unchanged
- **THEN** the page canvas does not request a new render pass for that page

### Requirement: Page rendering performance is measurable across large documents
The native GPU viewer SHALL provide a repeatable benchmark for page rendering at tens, hundreds, and thousands of pages.

#### Scenario: Benchmark covers representative page counts
- **WHEN** the viewer page rendering benchmark is run
- **THEN** it measures generated documents with 32, 256, and 2048 pages
- **AND** it reports initial full rendering, incremental no-op rendering, and incremental single-page rendering for each page count

#### Scenario: Benchmark covers page-list widget paint
- **WHEN** the viewer page rendering benchmark is run
- **THEN** it measures a Masonry page-list harness for 32, 256, and 2048 pages
- **AND** the harness includes preview frame merge, page scene update, in-place page widget update, and visible viewport rendering
- **AND** the harness reports update-only variants that do not include headless render readback
