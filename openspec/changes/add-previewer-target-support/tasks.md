## 1. Previewer Contract

- [x] 1.1 Add `PreviewTarget` and optional `supportedTargets` to the VS Code previewer provider contract.
- [x] 1.2 Resolve previewers against `tinymist.exportTarget`, including cache invalidation and fallback metadata for unsupported targets.
- [x] 1.3 Pass the selected preview target to extension document preview handlers.

## 2. Provider And Tests

- [x] 2.1 Declare the GPU viewer provider as paged-only.
- [x] 2.2 Add resolver coverage for unsupported target fallback and preserved default support.
- [x] 2.3 Run focused VS Code TypeScript/unit validation.
