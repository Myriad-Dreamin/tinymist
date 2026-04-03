## 1. Harden VS Code config loading

- [x] 1.1 Refactor `editors/vscode/src/config.ts` to validate `tinymist.fontPaths` before variable substitution and accept only arrays of strings.
- [x] 1.2 Reuse the guarded `fontPaths` handling for both activation-time config loading and `workspace/configuration` responses so malformed values are ignored instead of throwing.
- [x] 1.3 Add a deduplicated user-facing error message for malformed `tinymist.fontPaths` values that explains the expected `["fonts"]` JSON shape.

## 2. Validate the fix

- [x] 2.1 Add VS Code extension tests covering valid `fontPaths` arrays, singleton string values, and mixed-type arrays.
- [x] 2.2 Verify that Tinymist still activates and preserves variable substitution for valid arrays while ignoring malformed values without error-message spam.
