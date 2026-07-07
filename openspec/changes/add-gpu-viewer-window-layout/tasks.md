## 1. OpenSpec

- [x] 1.1 Add proposal, design, tasks, and previewer-provider spec delta for GPU viewer window layout.

## 2. GPU Viewer Provider

- [x] 2.1 Add `tinymist.gpuViewer.windowLayout` configuration with a side-by-side default and disabled opt-out.
- [x] 2.2 Add provider-side layout orchestration after spawning `tinymist-viewer`.
- [x] 2.3 Implement Windows, macOS, and Linux helper paths with soft-failure logging.
- [x] 2.4 Preserve viewer lifecycle behavior and keep preview task errors limited to launch failures.

## 3. Documentation And Validation

- [x] 3.1 Document layout configuration, platform support, and Linux/macOS prerequisites in the GPU viewer README.
- [x] 3.2 Run TypeScript type-checking for the GPU viewer extension.
