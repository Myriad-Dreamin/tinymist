## Why

Math-equation dot completion currently treats code-interpolated module-valued targets more narrowly than code and markup mode. For expressions such as `$ #calc. $`, tinymist still derives math-mode completion behavior from the cursor location and filters out the module's pure function exports, so users only see a partial completion list even though `#calc.` outside math mode surfaces the expected helpers. Issue `#2401` reports this inconsistency and shows it interfering with common math authoring flows that rely on `calc` functions inside equations.

## What Changes

- Preserve exported pure function members when completing code-interpolated, module-valued field access inside equations, including builtins such as `calc`.
- Keep the current math-mode behavior for non-module field access targets and postfix completions, such as symbol variants and postfix helpers on `arrow.`.
- Add regression coverage for math-mode module completion, including both full-member and prefix-filtered `calc.` cases, while keeping existing non-module math dot-access fixtures green.

## Capabilities

### New Capabilities
- `math-module-field-completion`: Surface module exports, including pure functions, when completing code-interpolated `module.member` expressions inside equations.

### Modified Capabilities
- None.

## Impact

- `crates/tinymist-query/src/analysis/completion/field_access.rs`
- Completion fixtures and snapshots under `crates/tinymist-query/src/fixtures/completion/`
