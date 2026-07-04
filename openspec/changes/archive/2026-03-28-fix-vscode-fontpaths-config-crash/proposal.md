## Why

Tinymist's VS Code extension currently crashes during activation if `tinymist.fontPaths` is configured as a string instead of a string array. A malformed setting should not take down the whole extension, especially when the extension can detect the problem and guide the user to the correct configuration shape.

## What Changes

- Make VS Code-side Tinymist configuration loading resilient to invalid `tinymist.fontPaths` values so extension activation continues instead of throwing `TypeError: paths.map is not a function`.
- Add explicit validation for `tinymist.fontPaths` before variable substitution so only arrays of strings are expanded as font paths.
- Surface an actionable error message that tells the user `tinymist.fontPaths` must be an array of strings and points them at the corrected JSON shape.
- Add coverage for the invalid-setting path so regressions are caught before release.

## Capabilities

### New Capabilities
- `vscode-config-loading`: loading Tinymist VS Code settings must remain resilient when user configuration contains invalid `tinymist.fontPaths` values.

### Modified Capabilities
- None.

## Impact

- `editors/vscode/src/config.ts` and extension activation flow that loads workspace configuration
- VS Code user-facing error reporting during startup or configuration changes
- VS Code extension tests covering malformed configuration input
