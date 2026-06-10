## Why

The native GPU viewer is now commonly launched as a side-by-side preview window. Its fit-to-window page layout still leaves a small visible gap, and every new viewer process starts from the default placement instead of the user's last working window shape.

## What Changes

- Make the viewer's automatic fit-width layout use the full available viewport width instead of keeping a fractional inset.
- Let editor clients persist the native viewer's last usable window size and position in their own storage.
- Add `tinymist-viewer` startup arguments for initial window size and position.
- Reuse the existing preview server path so `tinymist-viewer` can report valid window changes to tinymist server through a schema-versioned `viewer-window-state` data-plane message; tinymist server then forwards the state to the editor client.
- In the VS Code preview integration, use `globalState` for schema-versioned window state and pre-arrange side-by-side geometry before spawning the viewer when possible.

## Capabilities

### New Capabilities

- `native-viewer-window-state`: Native GPU viewer fit-to-window behavior, client-owned last-window-state persistence, and initial side-by-side placement.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist-viewer/src/main.rs` will accept initial geometry, observe native movement and resize events, and send schema-versioned `viewer-window-state` text messages over its existing data-plane websocket.
- `crates/typst-preview`, `crates/tinymist`, and `editors/vscode` will forward the viewer window-state message from tinymist server to the VS Code extension storage layer.
- `contrib/tinymist-gpu-viewer/editors/vscode/src/extension.ts` will accept initial window state from the main preview task and attempt side-by-side pre-layout before launching the viewer.
- Validation should include targeted Rust tests, VS Code provider type-checking, and formatting checks.
