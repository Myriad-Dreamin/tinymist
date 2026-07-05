## Why

The type VM and compile-before-check migration must be validated against real packages, not only focused fixtures. We need package-scale timing and typing-diff reports under `target/` to prove that precision and performance improve without recreating the previous timeout behavior.

## What Changes

- Add package-scale measurement for Typst knowledge/package data using existing local package inputs.
- Write all generated reports under `target/`.
- Compare baseline and current typings per package and per file.
- Measure file and package elapsed time, timeout count, VM cache behavior, and output precision changes.
- Produce reviewer-friendly summaries for stronger/weaker/changed typings.

## Capabilities

### New Capabilities
- `typst-knowledge-type-vm-benchmark`: Defines package-scale performance and typing-diff validation for the type VM pipeline.

### Modified Capabilities

## Impact

- Adds measurement tooling and report formats under `target/`.
- Does not vendor or checkout package registries into `/tmp`.
- Supports review of current PRs before and after low-level checker changes.
