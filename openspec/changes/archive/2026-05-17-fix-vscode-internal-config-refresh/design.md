## Context

Tinymist's VS Code extension currently builds the language-server configuration in two layers. `loadTinymistConfig` reads and normalizes user-facing settings from VS Code, including variable substitution and `tinymist.fontPaths` validation. After that, `configureEditorAndLanguage` mutates the startup config object to inject extension-managed fields such as the completion trigger flags, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`.

That split works at startup, but later configuration refreshes are sourced from VS Code again and do not automatically include the extension-managed fields because they are not normal user settings. On the Rust side, config updates treat absent keys as reset-to-default values, so a runtime settings change can silently drop the client/server capability contract that the session started with. Issue `#2378` calls out `delegate_fs_requests` and `supportHtmlInMarkdown`, but the same problem applies to the other extension-managed flags set only during startup.

This change spans the VS Code config-loading layer and the LSP synchronization path, so it benefits from a small design pass before implementation.

## Goals / Non-Goals

**Goals:**
- Keep extension-managed internal configuration values stable across startup and later workspace-configuration refreshes.
- Reuse a single source of truth for the extension-managed fields so new internal flags do not drift across code paths.
- Preserve the current normalization of user-facing settings, including variable substitution and malformed `tinymist.fontPaths` handling.
- Add focused regression tests around the shared config-shaping path.

**Non-Goals:**
- Changing the current values of the internal flags, including the existing `delegateFsRequests = false` behavior.
- Exposing the extension-managed fields as user-editable settings in VS Code.
- Changing Rust-side config defaults or deserialization behavior in this change.

## Decisions

### 1. Centralize extension-managed config augmentation in a shared helper

The canonical list of extension-managed fields should live in one shared VS Code-side helper instead of being hard-coded only in `configureEditorAndLanguage`. That helper should take a normalized Tinymist config object and apply the internal capability flags in one place.

Alternative considered:
- Keep mutating the config inline where it is first used. Rejected because it already caused drift between startup and later refresh paths, and future flags would be easy to miss again.

### 2. Reuse the same augmented config shape for every payload delivered to the server

The startup `initializationOptions` path and every later configuration synchronization path should reuse the same helper so the server sees the same extension-managed fields throughout the session. That includes the current `workspace/configuration` response path and any direct configuration-change payload path that supplies settings to the server.

Alternative considered:
- Patch only the startup path. Rejected because it preserves the current bug on later refreshes.
- Patch only one runtime path, such as `workspace/configuration`. Rejected because the extension should not depend on a single client-library delivery path staying unchanged.

### 3. Keep user-setting normalization separate from internal augmentation

User-facing settings should continue to flow through the existing normalization logic before the extension-managed fields are attached. This keeps `fontPaths` validation and VS Code variable substitution unchanged while making the internal booleans and capability flags deterministic.

Alternative considered:
- Fold the internal fields directly into the raw settings object before normalization. Rejected because it mixes two different responsibilities and makes the user-setting transformation logic harder to reason about.

### 4. Test the shared config-shaping boundary directly

The most durable regression coverage is to test the shared helper and at least one refresh-shaped config path that feeds the server after a settings change. This keeps the test surface small while still proving that runtime updates preserve the internal fields and existing user-setting processing.

Alternative considered:
- Rely only on manual verification or broad end-to-end extension tests. Rejected because the bug is narrowly about config payload shaping and is better caught with focused tests.

## Risks / Trade-offs

- [The VS Code client may deliver refreshed settings through more than one path] -> Mitigate by centralizing augmentation in a reusable helper and auditing every config payload path that reaches the language server.
- [Future internal flags could be added without coverage] -> Mitigate by keeping the full internal field list in one helper and asserting representative fields in tests.
- [Focused tests may not cover every editor-runtime edge case] -> Mitigate by testing both the shared helper and a refresh-oriented path instead of only unit-testing the raw normalization helpers.

## Migration Plan

1. Introduce a shared helper that augments a normalized Tinymist config object with the extension-managed internal fields.
2. Use that helper for startup config creation and later configuration synchronization.
3. Add focused VS Code tests that cover startup-shaped and refresh-shaped payloads.
4. Rollback, if needed, is a straightforward revert of the shared-helper usage because this change does not introduce new persisted data.

## Open Questions

- The implementation should confirm which VS Code language-client path currently carries the effective runtime settings object in this repository, but the shared-helper design intentionally supports either `workspace/configuration` responses or direct configuration-change payloads.
