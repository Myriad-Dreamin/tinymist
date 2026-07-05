## 1. Scan Runner

- [ ] 1.1 Add or update package scan command wiring to use local package data without writing package checkouts to `/tmp`.
- [ ] 1.2 Store run configuration, git SHA, and output directories under `target/tyck-package-scan/`.
- [ ] 1.3 Add timeout handling and resume support for large package sets.

## 2. Typing Reports

- [ ] 2.1 Emit machine-readable per-file typings and timing data.
- [ ] 2.2 Emit pretty per-package markdown reports split by package.
- [ ] 2.3 Include SCIP/API output hashes when available for behavior-preservation comparisons.

## 3. Diff Analysis

- [ ] 3.1 Compare baseline and current typings by package, file, symbol, and type text.
- [ ] 3.2 Classify changes into stronger, weaker, changed, unchanged, errors, and timeouts.
- [ ] 3.3 Generate summary tables for worst regressions, slowest files, and largest resultant/output sizes.

## 4. Validation

- [ ] 4.1 Add a small package subset smoke test for the scan tooling.
- [ ] 4.2 Document the manual full-corpus scan command and expected output paths.
- [ ] 4.3 Run the scanner before and after the type VM migration and attach summaries to review comments, not PR descriptions.
