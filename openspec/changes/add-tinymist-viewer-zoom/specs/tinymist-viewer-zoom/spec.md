## ADDED Requirements

### Requirement: Native viewer supports interactive zoom

The native `tinymist-viewer` SHALL allow users to zoom preview pages in, zoom pages out, and reset zoom to the fit-to-window scale.

#### Scenario: Viewer opens at fitted scale

- **WHEN** `tinymist-viewer` renders preview pages after launch
- **THEN** pages are scaled to fit the viewer width using the existing default behavior

#### Scenario: User zooms in with a shortcut

- **WHEN** the user activates a zoom-in shortcut in the native viewer
- **THEN** the viewer increases the rendered page scale above the fitted scale
- **AND** the viewer steps to the next supported zoom factor

#### Scenario: User zooms out with a shortcut

- **WHEN** the user activates a zoom-out shortcut in the native viewer
- **THEN** the viewer decreases the rendered page scale while keeping it within the supported zoom range
- **AND** the viewer steps to the previous supported zoom factor

#### Scenario: User zooms with modified wheel input

- **WHEN** the user scrolls the mouse wheel while holding a zoom modifier in the native viewer viewport
- **THEN** the viewer accumulates wheel distance before changing zoom
- **AND** the viewer handles the modified wheel event without also scrolling the page list
- **AND** the viewer accepts the modified wheel input even when the pointer is outside an individual page
- **AND** positive wheel delta zooms in while negative wheel delta zooms out

#### Scenario: Modified wheel zoom preserves the cursor anchor

- **WHEN** the user changes zoom with modified wheel input while the pointer is inside the viewer viewport
- **THEN** the viewer keeps the document content position under the pointer stable as far as scroll extents allow

#### Scenario: Zoomed-out pages are centered

- **WHEN** the rendered page list is narrower than the native viewer viewport
- **THEN** the viewer places the page list horizontally centered in the viewport
- **AND** the gray margins on the left and right sides have equal width
- **AND** modified-wheel zoom anchor calculations account for the centered page-list offset

#### Scenario: Viewer scrollbars remain available

- **WHEN** the rendered preview content is larger than the native viewer viewport
- **THEN** the viewer shows scrollbars for the overflowing axes
- **AND** the scrollbar positions stay synchronized with wheel scrolling and zoom anchor compensation

#### Scenario: User resets zoom

- **WHEN** the user activates the reset-zoom shortcut in the native viewer
- **THEN** the viewer returns rendered pages to the fit-to-window scale

### Requirement: Native viewer zoom preserves document coordinate mapping

The native `tinymist-viewer` SHALL continue to report source-click positions in document coordinates after pages are zoomed.

#### Scenario: User clicks a zoomed page

- **WHEN** the user clicks a rendered page while viewer zoom is not at the default value
- **THEN** the viewer maps the clicked page position through the rendered page bounds to the original document page coordinates
- **AND** the viewer sends the corresponding source-position request to the preview server
