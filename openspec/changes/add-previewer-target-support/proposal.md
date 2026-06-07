## Why

Custom previewer providers can currently claim all preview tasks once configured, even if they only support one Typst export target. This makes providers such as native/paged-only previewers break HTML-target workflows instead of letting Tinymist use the built-in previewer that can handle them.

## What Changes

- Add an optional target support declaration to the extension previewer provider contract.
- Let providers declare support for `paged`, `html`, or both targets.
- Resolve unsupported extension previewers by falling back to Tinymist's built-in previewer instead of failing the preview task.
- Pass the selected preview target to document preview handlers so providers can make target-aware decisions at launch time.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `previewer-provider`: Extension previewer providers can declare supported preview targets, and Tinymist falls back to the built-in previewer when the configured provider does not support the current target.

## Impact

- `editors/vscode/src/features/previewer.ts` gains the target support contract, resolver handling, and metadata.
- `editors/vscode/src/features/preview.ts` uses `tinymist.exportTarget` when resolving previewers and invalidates the previewer cache when it changes.
- `contrib/tinymist-gpu-viewer/editors/vscode/src/extension.ts` can declare that the current native viewer supports paged preview only.
- VS Code previewer-provider tests cover fallback behavior for unsupported targets.
