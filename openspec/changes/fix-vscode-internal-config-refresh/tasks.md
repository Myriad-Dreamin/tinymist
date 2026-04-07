## 1. Centralize extension-managed config shaping

- [x] 1.1 Move the extension-managed Tinymist config fields currently assigned only during startup into a shared helper or equivalent single source of truth.
- [x] 1.2 Keep that shared path responsible for the current internal fields, including the completion trigger flags, markdown/code-lens/code-action/show-document capability flags, and `delegateFsRequests`.
- [x] 1.3 Preserve existing VS Code variable substitution and malformed `tinymist.fontPaths` handling while applying the internal fields.

## 2. Reuse the shared config shape for runtime synchronization

- [x] 2.1 Use the shared augmented config shape for the startup `initializationOptions` payload.
- [x] 2.2 Use the same augmented config shape for later configuration synchronization from VS Code to the language server, including the current refresh path used after settings changes.
- [x] 2.3 Verify that changing an unrelated user setting no longer causes the server to lose extension-managed internal settings during the same session.

## 3. Add regression coverage

- [x] 3.1 Extend the VS Code config tests to assert the shared config-shaping path injects the internal settings while preserving `fontPaths` substitution and validation behavior.
- [x] 3.2 Add a refresh-oriented regression test that exercises the post-startup configuration payload delivered to the server and confirms the internal settings remain present.
- [x] 3.3 Run focused VS Code extension tests for the configuration-loading area.
