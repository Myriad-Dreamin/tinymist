## ADDED Requirements

### Requirement: Native viewer fit-width layout uses the full viewport width

The native GPU viewer SHALL fit pages to the available viewer viewport width without leaving an intentional fractional inset.

#### Scenario: Finite viewport width is preserved

- **WHEN** the viewer computes automatic fit width for a finite viewport width
- **THEN** the fit width equals that viewport width clamped only by the minimum supported width

#### Scenario: Invalid viewport width remains safe

- **WHEN** the viewer computes automatic fit width for a non-finite viewport width
- **THEN** the fit width falls back to a safe positive width

### Requirement: Native viewer accepts initial window state

The native GPU viewer SHALL accept client-provided initial window size and position and apply them to the native window attributes at startup.

#### Scenario: Client-provided state is applied on launch

- **WHEN** `tinymist-viewer` starts with `--initial-window-inner-size WIDTHxHEIGHT`
- **AND** `--initial-window-position X,Y` is provided
- **THEN** the viewer applies that inner size and position to the initial native window attributes

### Requirement: Native viewer reports window state changes through tinymist server

The native GPU viewer SHALL report valid window movement and resize changes to tinymist server through the existing preview data-plane websocket, and tinymist server SHALL forward the latest state to the editor client.

#### Scenario: Window changes are emitted

- **WHEN** `tinymist-viewer` is connected to the preview data-plane server
- **AND** the native viewer window is moved or resized to a valid size
- **THEN** the viewer sends a `viewer-window-state` text message with `schema_version: 1` and the latest window geometry to tinymist server
- **AND** tinymist server forwards the state to the editor client

#### Scenario: Direct viewer launches do not require a client

- **WHEN** `tinymist-viewer` starts without a reachable preview data-plane server
- **AND** the native viewer window is moved or resized
- **THEN** the viewer keeps running without requiring client-owned window-state persistence

### Requirement: VS Code preview integration owns window state persistence

The VS Code preview integration SHALL persist and restore viewer window state through VS Code extension storage instead of a viewer-owned state file.

#### Scenario: Stored state is restored on launch

- **WHEN** VS Code extension storage contains a valid schema-versioned viewer window-state record
- **AND** the GPU viewer provider launches `tinymist-viewer` through the previewer task contract
- **THEN** the preview integration passes the stored geometry to the provider
- **AND** the provider passes the geometry through the viewer's initial window arguments

#### Scenario: Server-forwarded window messages update extension storage

- **WHEN** the preview integration receives a valid schema-versioned viewer window-state notification from tinymist server
- **THEN** the integration writes that state to VS Code extension storage for future launches

#### Scenario: Rapid resize updates are stored in notification order

- **WHEN** the preview integration receives multiple valid viewer window-state notifications from one resize gesture
- **AND** earlier extension-storage writes complete slower than later writes
- **THEN** the stored viewer window state still reflects the latest notification arrival order

#### Scenario: Positionless updates preserve stored position

- **WHEN** VS Code extension storage contains a valid viewer window-state record with position
- **AND** the preview integration receives a valid window-state notification with size but without position
- **THEN** the integration updates the stored size
- **AND** keeps the previous stored position

#### Scenario: Incompatible stored state is ignored

- **WHEN** the stored viewer window-state record has an unsupported schema version or invalid geometry
- **THEN** the provider ignores the record and launches the viewer without that stored geometry

### Requirement: VS Code side-by-side launch pre-arranges windows

The VS Code GPU viewer provider SHALL attempt to prepare side-by-side geometry before spawning the native viewer when side-by-side layout is enabled.

#### Scenario: Stored state keeps viewer geometry while pre-layout still runs

- **WHEN** `tinymist.gpuViewer.windowLayout` is `sideBySide`
- **AND** VS Code extension storage contains a valid schema-versioned viewer window-state record
- **THEN** the provider still attempts side-by-side pre-layout before launch
- **AND** passes the stored geometry as the viewer's initial window size and position
- **AND** skips the post-spawn viewer layout repair pass

#### Scenario: Side-by-side pre-layout succeeds

- **WHEN** `tinymist.gpuViewer.windowLayout` is `sideBySide`
- **AND** no valid stored viewer window state is available
- **AND** the provider can compute and apply the platform work-area split before launch
- **THEN** the provider moves VS Code to the left side
- **AND** passes the right-side rectangle as the viewer's initial window size and position

#### Scenario: Side-by-side pre-layout fails

- **WHEN** `tinymist.gpuViewer.windowLayout` is `sideBySide`
- **AND** no valid stored viewer window state is available
- **AND** the provider cannot compute or apply side-by-side geometry before launch
- **THEN** the provider falls back to default placement
- **AND** the post-spawn side-by-side repair pass can still arrange the windows
