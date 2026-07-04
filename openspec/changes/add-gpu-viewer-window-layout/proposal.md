## Why

The Tinymist GPU Viewer provider launches a native preview window outside VS Code. Users who use it as their primary Typst preview often want the same predictable editing layout they get from a VS Code preview tab: source on the left and preview on the right. Today the provider starts the viewer process but leaves both the VS Code and viewer windows wherever the operating system or window manager places them.

We need an opt-in desktop layout helper for the native viewer provider so opening preview can automatically arrange VS Code and the viewer side by side without changing Tinymist's preview server protocol or forcing webview-based previewers to participate.

## What Changes

- Add a `tinymist.gpuViewer.windowLayout` setting for the GPU viewer provider.
- Enable side-by-side layout by default for the GPU viewer provider, with a setting to disable it.
- After launching `tinymist-viewer`, run a platform-specific layout helper that places the VS Code window on the left and the viewer window on the right.
- Support Windows through Win32 window APIs invoked from PowerShell, macOS through AppleScript, and Linux through common EWMH/X11 helpers when available.
- Log layout failures to the GPU viewer output channel without failing the preview task.

## Capabilities

### New Capabilities

- `gpu-viewer-window-layout`: Let the Tinymist GPU Viewer provider arrange the active VS Code window and native GPU viewer window side by side after preview launch, with an explicit opt-out setting.

### Modified Capabilities

- `previewer-provider`: Native document preview handlers may perform provider-local desktop window arrangement after Tinymist starts the preview server and calls `handlePreview`.

## Impact

- `contrib/tinymist-gpu-viewer/editors/vscode/src/extension.ts` will gain provider-local window layout orchestration.
- `contrib/tinymist-gpu-viewer/editors/vscode/package.json` will expose the layout setting.
- `contrib/tinymist-gpu-viewer/editors/vscode/README.md` will document prerequisites and platform behavior.
- Validation should cover TypeScript compilation for the GPU viewer extension.
