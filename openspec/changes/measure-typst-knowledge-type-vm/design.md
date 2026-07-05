## Context

Previous package scans exposed timeouts and typing regressions. JSONL-only output was hard to inspect, and generated data must not inflate git history. The measurement workflow needs deterministic per-package reports and enough detail to explain changed typings.

The package corpus is large, so the tool must support incremental runs, resume behavior, timeouts, and summaries without requiring sparse checkouts in temporary directories.

## Goals / Non-Goals

**Goals:**
- Measure package-level and file-level type-check timing.
- Compare baseline and current quoted typings.
- Classify typing changes as stronger, weaker, changed, or formatting-only where possible.
- Store reports under `target/`.
- Detect timeout regressions and cache behavior changes.

**Non-Goals:**
- Commit generated package scan outputs.
- Fetch multi-gigabyte sparse checkouts into `/tmp`.
- Decide semantic correctness solely from automated classification.

## Decisions

- Use existing local package data or registry fetch paths configured by the repo; avoid writing package data to `/tmp`.
- Emit both machine-readable and pretty per-package reports. Pretty reports make review possible without opening giant JSONL files.
- Use per-package directories under `target/tyck-package-scan/` so reports can be inspected and cleaned independently.
- Include hashes of SCIP/API outputs when comparing behavior across optimization runs.
- Treat timeout absence as a first-class acceptance criterion, separate from precision diffs.

## Risks / Trade-offs

- [Risk] Full package scans can take too long for normal CI. -> Mitigation: keep the full scan manual and add a small smoke subset for CI.
- [Risk] Automated stronger/weaker classification can be wrong. -> Mitigation: preserve raw before/after typings and include examples in summaries.
- [Risk] Generated reports can become huge. -> Mitigation: split by package and keep everything under `target/`.
- [Risk] Baseline revisions may be lost. -> Mitigation: record git SHAs, command config, and output hashes with each run.
