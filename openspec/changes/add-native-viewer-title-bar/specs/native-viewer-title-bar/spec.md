## ADDED Requirements

### Requirement: Viewer-Owned Native Title Bar

The native GPU viewer SHALL render a viewer-owned title bar when running in its native window.

#### Scenario: Title bar shows document-aware window title

- **GIVEN** the GPU viewer provider launches `tinymist-viewer` for a Typst document
- **WHEN** the viewer window is created
- **THEN** the visible window title SHALL include the document title and `Tinymist View`
- **AND** reconnecting or stopped states MAY append a status suffix.

#### Scenario: Title bar provides expected window controls

- **GIVEN** the native viewer window is visible
- **WHEN** the user interacts with title-bar controls
- **THEN** the viewer SHALL support help, minimize, maximize, and close actions.

### Requirement: Native Window Interaction For Decorationless Viewer

The native GPU viewer SHALL preserve expected native window interactions for decorationless windows where the platform supports them.

#### Scenario: Windows title-bar hit testing preserves drag and resize

- **GIVEN** the viewer runs on Windows
- **WHEN** the user points at the title-bar drag region or window resize edges
- **THEN** the viewer SHALL expose the matching native non-client hit-test result.

#### Scenario: Windows maximize button preserves snap behavior

- **GIVEN** the viewer runs on Windows
- **WHEN** the user hovers or clicks the custom maximize button
- **THEN** the viewer SHALL expose native maximize-button hit testing so platform snap-layout behavior can remain available.

### Requirement: Viewer Help Overlay

The native GPU viewer SHALL expose preview interaction help from the viewer title bar.

#### Scenario: User toggles preview help

- **GIVEN** the native viewer window is visible
- **WHEN** the user activates the help title-bar control
- **THEN** the viewer SHALL toggle an in-app overlay describing preview interactions.
