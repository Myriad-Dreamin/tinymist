## ADDED Requirements

### Requirement: Native GPU viewer arranges windows side by side by default

The Tinymist GPU Viewer previewer provider SHALL arrange the active VS Code window and native GPU viewer window side by side after launching a document preview unless window layout is disabled.

#### Scenario: Default side-by-side layout moves VS Code and viewer

- **WHEN** `tinymist.gpuViewer.windowLayout` is unset or set to `sideBySide`
- **AND** the GPU viewer provider launches `tinymist-viewer` for a preview task
- **THEN** the provider attempts to place the VS Code window on the left side of the primary work area
- **AND** the provider attempts to place the native GPU viewer window on the right side of the primary work area

#### Scenario: Layout disabled preserves operating system placement

- **WHEN** `tinymist.gpuViewer.windowLayout` is set to `disabled`
- **THEN** the provider launches `tinymist-viewer` without moving the VS Code or viewer windows

#### Scenario: Layout helper failure does not fail preview

- **WHEN** side-by-side layout is enabled
- **AND** the operating system, desktop environment, permissions, or helper command prevents window movement
- **THEN** the provider logs the layout failure
- **AND** the preview task remains running
