## Why

The native GPU viewer runs outside editor chrome. With the operating-system title bar, the viewer has generic window controls and no place for preview-specific controls such as help. It also cannot align its viewport chrome with the preview content background.

On Windows, removing native decorations without replacing non-client hit testing would lose expected resize, drag, maximize, and snap-layout behavior. The viewer needs a custom title bar that keeps native window interactions intact where the platform supports them.

## What Changes

- Make `tinymist-viewer` use a decorationless native window with a viewer-owned title bar.
- Add title-bar controls for help, minimize, maximize, and close.
- Add an in-app help overlay for preview keyboard and mouse interactions.
- Preserve Windows native title-bar hit testing for drag, resize, maximize, and snap-layout hover behavior.
- Let the GPU viewer provider pass a document title so native window search still works when the visible viewer title includes the document name.
- Align the preview viewport background and scrollbar styling with the custom chrome.

## Capabilities

### New Capabilities

- `native-viewer-title-bar`: GPU viewer-owned title bar, help overlay, and native window hit testing for decorationless windows.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist-viewer` gains custom title-bar widgets, native Windows hit testing, and tests for the new window chrome behavior.
- `contrib/tinymist-gpu-viewer/editors/vscode` passes a document title to `tinymist-viewer` and finds the viewer window by the updated title.
- Validation should include targeted `tinymist-viewer` tests, Rust formatting, and GPU viewer provider type-checking when TypeScript dependencies are installed.
