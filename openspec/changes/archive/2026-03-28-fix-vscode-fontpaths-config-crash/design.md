## Context

The VS Code extension loads the full `tinymist` workspace configuration during activation and again when the language client answers `workspace/configuration` requests. Both paths reuse `substVscodeVarsInConfig` in [`editors/vscode/src/config.ts`](/home/kamiyoru/work/rust/tinymist/editors/vscode/src/config.ts), and that helper currently assumes `tinymist.fontPaths` is always a string array. When a user sets `tinymist.fontPaths` to a singleton string, the helper calls `.map()` on a non-array value and activation aborts before the extension can show anything more helpful than a generic failure toast.

This change is small in surface area but slightly cross-cutting in behavior: the same normalization logic feeds initial activation and later configuration refreshes. The design therefore needs to keep those paths consistent and avoid replacing one crash with repeated error-message spam.

## Goals / Non-Goals

**Goals:**
- Preserve successful activation when `tinymist.fontPaths` is malformed.
- Keep the configuration contract aligned with existing docs and package metadata: `tinymist.fontPaths` is an array of strings or `null`.
- Provide an actionable user-facing message that explains the invalid shape and shows the expected JSON form.
- Preserve existing variable-substitution behavior for valid string-array values.
- Cover the malformed-setting path with extension tests.

**Non-Goals:**
- Broadening `tinymist.fontPaths` to officially support singleton strings.
- Redesigning Tinymist's overall configuration loading model.
- Validating every Tinymist setting in this change.
- Changing server-side font path semantics beyond shielding it from malformed VS Code input.

## Decisions

### 1. Centralize `fontPaths` validation in the shared VS Code config substitution path

The extension should validate `tinymist.fontPaths` in the same helper that already performs VS Code variable substitution. That keeps activation-time config loading and later `workspace/configuration` responses consistent, and it avoids duplicating normalization rules in multiple call sites.

Alternative considered:
- Validate only in `loadTinymistConfig()`. Rejected because `substVscodeVarsInConfig()` is also used by the LSP middleware path, so a single validation point is safer and easier to test.

### 2. Treat malformed `fontPaths` values as invalid input to ignore, not as values to coerce

If `tinymist.fontPaths` is not an array of strings, Tinymist should ignore that setting value for the current load and continue activation with default font-path behavior. This preserves the documented contract and avoids silently expanding accepted input types that the rest of the extension and docs do not promise.

Alternative considered:
- Coerce a singleton string into a one-element array. Rejected because it turns a user mistake into an undocumented supported shape and leaves ambiguity about how to handle other malformed values such as numbers, objects, or mixed-type arrays.

### 3. Show a deduplicated actionable error message for malformed `fontPaths`

When Tinymist ignores a malformed `fontPaths` value, the extension should surface an error message that names the setting and shows the expected JSON shape, for example `{\"tinymist.fontPaths\": [\"fonts\"]}`. The error message should be deduplicated so repeated config reads during the same session do not spam the user.

Alternative considered:
- Throw an activation error. Rejected because the goal is to preserve extension availability.
- Only log to the console. Rejected because the current problem is specifically that users do not get a descriptive recovery path.

### 4. Add targeted tests around both valid and invalid `fontPaths` inputs

The extension should add focused tests for config normalization covering at least:
- valid string arrays with variable substitution intact
- singleton string values
- arrays containing non-string members

This is the smallest test surface that proves the fix and guards against reintroducing the crash in future config refactors.

Alternative considered:
- Rely on manual verification only. Rejected because this regression lives in a low-level helper that can easily be broken again during unrelated configuration work.

## Risks / Trade-offs

- [Ignoring invalid `fontPaths` means custom fonts stay disabled until the user fixes settings] -> Mitigate with a clear error message that explains the exact JSON shape required.
- [Deduplication could hide repeated configuration problems after the first error message] -> Mitigate by keeping a console log alongside the user-facing error message for later inspection.
- [Tests may require extra mocking around VS Code configuration objects] -> Mitigate by testing the pure normalization helper as directly as possible.

## Migration Plan

1. Refactor `editors/vscode/src/config.ts` so `fontPaths` validation and substitution happen through a shared guarded helper.
2. Update the activation and configuration-response flow to reuse the guarded result without throwing on malformed input.
3. Add tests for valid and invalid `fontPaths` inputs.
4. Verify that Tinymist still activates when `tinymist.fontPaths` is a string and that the error message explains the fix.

## Open Questions

- Should this change also tighten the VS Code contribution metadata for `tinymist.fontPaths` by declaring `items.type = "string"` so the editor can catch mixed-type arrays earlier?
