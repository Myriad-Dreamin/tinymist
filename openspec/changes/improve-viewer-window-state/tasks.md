## 1. OpenSpec

- [x] 1.1 Add proposal, design, tasks, and native-viewer-window-state spec.
- [x] 1.2 Update artifacts for client-owned storage, server-forwarded viewer window-state messages, and side-by-side pre-layout.

## 2. Viewer Layout

- [x] 2.1 Remove the fractional fit-width inset so auto-fit pages occupy the full viewer viewport.
- [x] 2.2 Add targeted tests for fit-width behavior.

## 3. Window State Persistence

- [x] 3.1 Add viewer CLI arguments for initial window size and position.
- [x] 3.2 Observe native move/resize events and send schema-versioned window-state messages through tinymist server.
- [x] 3.3 Store and restore window state in the VS Code preview integration with `globalState`.
- [x] 3.4 Add side-by-side pre-layout before spawning the viewer, with post-spawn repair retained.
- [x] 3.5 Add targeted tests for geometry parsing, validation, and window-state payloads.
- [x] 3.6 Keep side-by-side pre-layout active while preserving stored viewer geometry.
- [x] 3.7 Preserve stored window position across positionless size updates.
- [x] 3.8 Serialize VS Code window-state storage writes in notification order.

## 4. Validation

- [x] 4.1 Run targeted `tinymist-viewer` tests.
- [x] 4.2 Run VS Code GPU viewer type-checking.
- [x] 4.3 Run formatting and diff checks.
